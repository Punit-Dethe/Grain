// External MCP peer only. The real app owns discovery, validation and dispatch.
import assert from "node:assert/strict";
import { createServer } from "node:https";
import { readFile } from "node:fs/promises";
import { join } from "node:path";
import { randomUUID } from "node:crypto";

export const MCP_EXTENSION_ID = "mcp.grain-harness";
export const MCP_INPUT = {
  query: { text: 'Grain caf\u00e9 "quoted"\nline', limit: 3, enabled: true },
  tags: ["one", "two"],
  optional: null,
};
export function catalog(revision = "one") {
  return [
    {
      name: "fixture_read",
      description: `Harness MCP nested read (${revision})`,
      inputSchema: {
        type: "object",
        additionalProperties: false,
        required: ["query", "tags"],
        properties: {
          query: {
            type: "object",
            additionalProperties: false,
            required: ["text", "limit", "enabled"],
            properties: {
              text: { type: "string" },
              limit: { type: "integer", minimum: 0 },
              enabled: { type: "boolean" },
            },
          },
          tags: { type: "array", items: { type: "string" }, maxItems: 4 },
          optional: { type: ["string", "null"] },
        },
      },
    },
    {
      name: "excluded_remote",
      description: "Harness MCP excluded remote reference",
      inputSchema: {
        type: "object",
        properties: {
          value: { $ref: "https://grain-agent-harness.invalid/private-schema" },
        },
      },
    },
    {
      name: "fixture_other",
      description: "Harness MCP unrelated read",
      inputSchema: { type: "object", properties: {} },
    },
    {
      name: "excluded_dialect",
      description: "Harness MCP excluded dialect",
      inputSchema: {
        $schema: "https://grain-agent-harness.invalid/dialect",
        type: "object",
        properties: {},
      },
    },
  ];
}

export async function startMcpFixture(
  root,
  { wrongNestedType = false, supportedExcluded = false } = {},
) {
  const [cert, key] = await Promise.all([
    readFile(join(root, "mcp-tls/cert.pem")),
    readFile(join(root, "mcp-tls/key.pem")),
  ]);
  const journal = [],
    sockets = new Set(),
    sessions = new Set();
  let mode = {
    lifecycle: "stateless",
    reply: "json",
    catalog: "mixed",
    revision: "one",
  };
  function record(entry) {
    assert.ok(journal.length < 512, "MCP fixture journal overflow");
    journal.push(entry);
  }
  function json(res, status, value, headers = {}) {
    res.writeHead(status, {
      "content-type": "application/json",
      "cache-control": "no-store",
      ...headers,
    });
    res.end(JSON.stringify(value));
  }
  const server = createServer({ key, cert }, async (req, res) => {
    try {
      assert.equal(req.url, "/mcp");
      assert.equal(
        req.headers.authorization,
        undefined,
        "Credentials reached an unauthenticated fixture",
      );
      const session = req.headers["mcp-session-id"];
      if (req.method === "DELETE") {
        assert.ok(sessions.delete(session), "Unknown session cleanup");
        record({ method: "DELETE", phase: "session-deleted" });
        res.writeHead(204);
        res.end();
        return;
      }
      if (req.method === "GET") {
        // No unsolicited server notifications/subscriptions in this fixture.
        record({ method: "GET", phase: "stream-refused" });
        res.writeHead(405);
        res.end();
        return;
      }
      assert.equal(req.method, "POST");
      let size = 0;
      const chunks = [];
      for await (const chunk of req) {
        size += chunk.length;
        assert.ok(size <= 64 * 1024, "MCP fixture request exceeded bound");
        chunks.push(chunk);
      }
      const body = JSON.parse(Buffer.concat(chunks).toString("utf8"));
      assert.equal(body.jsonrpc, "2.0");
      assert.equal(typeof body.method, "string");
      const entry = {
        method: body.method,
        lifecycle: mode.lifecycle,
        reply: mode.reply,
      };
      record(entry);
      if (body.id === undefined) {
        assert.equal(body.method, "notifications/initialized");
        assert.ok(sessions.has(session));
        res.writeHead(202);
        res.end();
        return;
      }
      const headers = {};
      let result;
      if (body.method === "server/discover") {
        if (mode.lifecycle === "legacy") {
          json(res, 200, {
            jsonrpc: "2.0",
            id: body.id,
            error: { code: -32601, message: "Method not found" },
          });
          return;
        }
        result = {
          resultType: "complete",
          supportedVersions: ["2026-07-28"],
          capabilities: { tools: {} },
          ttlMs: 0,
          cacheScope: "private",
        };
      } else if (body.method === "initialize") {
        assert.equal(mode.lifecycle, "legacy");
        const id = randomUUID();
        sessions.add(id);
        headers["mcp-session-id"] = id;
        result = {
          protocolVersion: "2025-11-25",
          capabilities: { tools: {} },
          serverInfo: { name: "Grain controlled MCP", version: "1" },
        };
      } else {
        if (mode.lifecycle === "legacy")
          assert.ok(
            sessions.has(session),
            "Request escaped its negotiated session",
          );
        else {
          assert.equal(session, undefined);
          const meta = body.params?._meta;
          assert.equal(
            meta?.["io.modelcontextprotocol/protocolVersion"],
            "2026-07-28",
          );
          assert.equal(
            meta?.["io.modelcontextprotocol/clientInfo"]?.name,
            "grain",
          );
          entry.requestMetadataVerified = true;
        }
        if (body.method === "tools/list") {
          const tools = catalog(mode.revision);
          if (supportedExcluded)
            tools[1].inputSchema = { type: "object", properties: {} };
          if (mode.catalog === "unsupported")
            result = {
              tools: tools.filter((tool) => tool.name.startsWith("excluded_")),
            };
          else {
            const cursor = body.params?.cursor;
            assert.ok(
              cursor === undefined || cursor === "page-two",
              "Unexpected paging cursor",
            );
            entry.page = cursor ? 2 : 1;
            result = cursor
              ? { tools: tools.slice(2) }
              : { tools: tools.slice(0, 2), nextCursor: "page-two" };
          }
          if (mode.catalog === "repeat_cursor") result.nextCursor = "page-two";
        } else if (body.method === "tools/call") {
          entry.tool = body.params.name;
          assert.equal(
            entry.tool,
            "fixture_read",
            "An excluded/unselected tool reached the provider",
          );
          assert.deepEqual(
            body.params.arguments,
            MCP_INPUT,
            "Actual MCP wire argument types changed",
          );
          entry.argumentsVerified = true;
          const actual = structuredClone(body.params.arguments);
          if (wrongNestedType) actual.query.limit = String(actual.query.limit);
          result = {
            content: [
              {
                type: "text",
                text: `Harness MCP reply: ${JSON.stringify(actual)}`,
              },
            ],
            isError: false,
          };
        } else throw new Error(`Unexpected MCP method: ${body.method}`);
      }
      const message = { jsonrpc: "2.0", id: body.id, result };
      if (mode.reply === "sse") {
        res.writeHead(200, {
          "content-type": "text/event-stream",
          "cache-control": "no-store",
          ...headers,
        });
        res.end(`event: message\ndata: ${JSON.stringify(message)}\n\n`);
      } else json(res, 200, message, headers);
    } catch (error) {
      record({ phase: "error", error: String(error.message).slice(0, 300) });
      if (!res.headersSent)
        json(res, 500, { error: "Controlled MCP request failed" });
      else res.destroy();
    }
  });
  server.on("connection", (socket) => {
    sockets.add(socket);
    socket.once("close", () => sockets.delete(socket));
  });
  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolve);
  });
  return {
    port: server.address().port,
    journal,
    get activeSessions() {
      return sessions.size;
    },
    configure(next) {
      assert.equal(
        sessions.size,
        0,
        "Fixture reconfigured with a live session",
      );
      assert.ok(
        Object.keys(next).every((key) =>
          ["lifecycle", "reply", "catalog", "revision"].includes(key),
        ),
      );
      const candidate = { ...mode, ...next };
      assert.ok(["stateless", "legacy"].includes(candidate.lifecycle));
      assert.ok(["json", "sse"].includes(candidate.reply));
      assert.ok(
        ["mixed", "unsupported", "repeat_cursor"].includes(candidate.catalog),
      );
      assert.ok(["one", "two"].includes(candidate.revision));
      mode = candidate;
    },
    async close() {
      const pending = new Promise((resolve) => server.close(resolve));
      for (const socket of sockets) socket.destroy();
      await pending;
      assert.equal(sockets.size, 0, "MCP fixture retained sockets");
      assert.equal(sessions.size, 0, "MCP fixture retained protocol sessions");
    },
  };
}
