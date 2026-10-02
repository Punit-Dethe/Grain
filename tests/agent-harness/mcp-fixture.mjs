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
  {
    wrongNestedType = false,
    supportedExcluded = false,
    shortPreview = false,
  } = {},
) {
  const [cert, key] = await Promise.all([
    readFile(join(root, "mcp-tls/cert.pem")),
    readFile(join(root, "mcp-tls/key.pem")),
  ]);
  const journal = [],
    sockets = new Set(),
    sessions = new Set(),
    held = new Set(),
    closedReplies = [];
  let configuredMode = {
    lifecycle: "stateless",
    reply: "json",
    catalog: "mixed",
    revision: "one",
    result: "normal",
  };
  function record(entry) {
    assert.ok(journal.length < 2048, "MCP fixture journal overflow");
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
      // A held reply must keep the mode of its own request.
      const mode = { ...configuredMode };
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
        result: mode.result,
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
          if (mode.result === "preview") {
            result.content[0].text =
              "Harness MCP large: " + "\u00e9".repeat(32768);
            result.structuredContent = { omitted: "x".repeat(32768) };
            if (shortPreview) {
              result.content[0].text = "Harness MCP large: small";
              delete result.structuredContent;
            }
          }
          if (mode.result === "held") {
            res.writeHead(200, {
              "content-type":
                mode.reply === "sse" ? "text/event-stream" : "application/json",
            });
            res.flushHeaders();
            const handle = {
              res,
              message: { jsonrpc: "2.0", id: body.id, result },
              reply: mode.reply,
            };
            held.add(handle);
            entry.phase = "held";
            res.once("close", () => {
              held.delete(handle);
              assert.ok(
                closedReplies.length < 4,
                "Unreleased closed reply handles",
              );
              closedReplies.push(handle);
              record({
                phase: "held-closed",
                lifecycle: mode.lifecycle,
                reply: mode.reply,
              });
            });
            return;
          }
          if (mode.result === "drop") {
            entry.phase = "dropped-after-dispatch";
            res.destroy();
            return;
          }
          if (mode.result.startsWith("overflow_")) {
            const kind = mode.result.slice("overflow_".length);
            const sse = kind.startsWith("sse");
            const size =
              kind === "http_error"
                ? 17 * 1024
                : sse
                  ? 513 * 1024
                  : 2 * 1024 * 1024 + 1;
            // Valid JSON/SSE envelopes isolate byte-limit failures from parse
            // failures. Many small comments isolate the raw stream budget.
            const oversized = {
              jsonrpc: "2.0",
              id: body.id,
              result: { content: [{ type: "text", text: "x".repeat(size) }] },
            };
            const payload =
              kind === "sse_comments"
                ? (":" + "x".repeat(1020) + "\n\n").repeat(515) +
                  `event: message\ndata: ${JSON.stringify({ jsonrpc: "2.0", id: body.id, result })}\n\n`
                : kind === "sse_data"
                  ? `event: message\ndata: ${JSON.stringify(oversized)}\n\n`
                  : JSON.stringify(oversized);
            res.writeHead(kind === "http_error" ? 500 : 200, {
              "content-type": sse ? "text/event-stream" : "application/json",
              ...(kind === "json_declared"
                ? { "content-length": String(Buffer.byteLength(payload)) }
                : {}),
            });
            entry.phase = "overflow-sent";
            entry.wireBytes = Buffer.byteLength(payload);
            // Explicit chunked framing exercises incremental limits, even if
            // the OS coalesces writes. No timer or background flood survives.
            res.write(payload.slice(0, 4096));
            res.end(payload.slice(4096));
            return;
          }
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
    get heldCalls() {
      return held.size;
    },
    attemptLateReply() {
      // Deliberately misbehaving peer: try to finish a closed request. Never
      // retain a response handle beyond this operation.
      assert.ok(
        closedReplies.length > 0 && held.size === 0,
        "Late reply attempted before disconnect",
      );
      for (const { res, message, reply } of closedReplies.splice(0)) {
        assert.equal(res.destroyed, true, "Cancelled reply still writable");
        res.end(
          reply === "sse"
            ? `event: message\ndata: ${JSON.stringify(message)}\n\n`
            : JSON.stringify(message),
        );
        record({ phase: "late-reply-discarded", destroyed: res.destroyed });
      }
    },
    configure(next) {
      assert.equal(
        sessions.size,
        0,
        "Fixture reconfigured with a live session",
      );
      assert.ok(
        Object.keys(next).every((key) =>
          ["lifecycle", "reply", "catalog", "revision", "result"].includes(key),
        ),
      );
      assert.equal(held.size, 0, "Fixture reconfigured with a held call");
      const candidate = { ...configuredMode, ...next };
      assert.ok(["stateless", "legacy"].includes(candidate.lifecycle));
      assert.ok(["json", "sse"].includes(candidate.reply));
      assert.ok(
        ["mixed", "unsupported", "repeat_cursor"].includes(candidate.catalog),
      );
      assert.ok(["one", "two"].includes(candidate.revision));
      assert.ok(
        [
          "normal",
          "preview",
          "held",
          "drop",
          "overflow_json_declared",
          "overflow_json_chunked",
          "overflow_sse_data",
          "overflow_sse_comments",
          "overflow_http_error",
        ].includes(candidate.result),
      );
      configuredMode = candidate;
    },
    async close() {
      const pending = new Promise((resolve) => server.close(resolve));
      for (const socket of sockets) socket.destroy();
      await pending;
      closedReplies.length = 0;
      assert.equal(sockets.size, 0, "MCP fixture retained sockets");
      assert.equal(sessions.size, 0, "MCP fixture retained protocol sessions");
      assert.equal(held.size, 0, "MCP fixture retained held replies");
    },
  };
}
