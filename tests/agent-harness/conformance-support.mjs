// Test-only boundary to the pinned official server; no alternate MCP client.
import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { createServer } from "node:https";
import { request } from "node:http";
import { readFile } from "node:fs/promises";
import { join } from "node:path";

const exec = promisify(execFile);
export const CONFORMANCE_VERSION = "0.2.0-alpha.12";
export const cases = [
  {
    id: "mcp.conformance-initialize",
    scenario: "initialize",
    version: "2025-11-25",
    check: "mcp-client-initialization",
  },
  {
    id: "mcp.conformance-tools-legacy",
    scenario: "tools_call",
    version: "2025-11-25",
    check: "tool-add-numbers",
  },
  {
    id: "mcp.conformance-tools-modern",
    scenario: "tools_call",
    version: "2026-07-28",
    check: "tool-add-numbers",
  },
];

export function localServerUrl(value, scenario) {
  const url = new URL(value);
  assert.ok(
    ["localhost", "127.0.0.1"].includes(url.hostname),
    "Official server must be exact loopback",
  );
  assert.equal(url.protocol, "http:");
  assert.ok(Number(url.port) >= 1024 && Number(url.port) <= 65535);
  assert.equal(url.username + url.password + url.search + url.hash, "");
  assert.equal(url.pathname, scenario === "initialize" ? "/" : "/mcp");
  return url;
}

export async function assertServerOwner(port, pid) {
  assert.equal(
    process.platform,
    "win32",
    "Real WebView2 conformance needs Windows",
  );
  assert.ok(Number.isSafeInteger(pid) && pid > 0);
  const { stdout } = await exec("netstat.exe", ["-ano", "-p", "TCP"], {
    windowsHide: true,
    timeout: 5000,
    maxBuffer: 2 * 1024 * 1024,
  });
  const listeners = stdout
    .split(/\r?\n/)
    .map((line) => line.trim().split(/\s+/))
    .filter(
      (fields) =>
        fields[0] === "TCP" &&
        fields[1]?.endsWith(`:${port}`) &&
        fields[3] === "LISTENING",
    );
  assert.ok(
    listeners.length > 0 &&
      listeners.every((fields) => Number(fields[4]) === pid),
    "Official server listener is not owned by this runner",
  );
}

export function verifyOfficialChecks(checks, testCase) {
  assert.ok(
    Array.isArray(checks) && checks.length > 0,
    "Official checks are absent or skipped",
  );
  assert.ok(
    checks.every((check) => ["SUCCESS", "INFO"].includes(check.status)),
    "Official checks contain failure, warning or skipped results",
  );
  const required = checks.filter((check) => check.id === testCase.check);
  assert.ok(
    required.length > 0 &&
      required.every((check) => check.status === "SUCCESS"),
    "Required official check was not successful",
  );
  if (testCase.scenario === "tools_call") {
    const wire = checks.filter((check) => check.id === "wire-schema-valid");
    assert.ok(
      wire.length === 1 &&
        wire[0].status === "SUCCESS" &&
        Number.isSafeInteger(wire[0].details?.messagesValidated) &&
        wire[0].details.messagesValidated >= 3 &&
        Array.isArray(wire[0].details.violations) &&
        wire[0].details.violations.length === 0,
      "Official wire-schema evidence is missing or incomplete",
    );
    assert.ok(
      required.every(
        (check) =>
          check.details?.a === 5 &&
          check.details?.b === 3 &&
          check.details?.result === 8,
      ),
      "Official tool check did not verify exact numeric inputs/result",
    );
  } else {
    assert.ok(
      required.every(
        (check) =>
          check.details?.clientName === "grain" &&
          check.details?.protocolVersionSent === testCase.version,
      ),
      "Official initialize check did not observe Grain's expected wire identity/version",
    );
  }
}

export function initializeFixtureBlocked(application, checks) {
  const requests = application.mcpFixture?.requests;
  return (
    application.cleanup?.status === "Pass" &&
    application.results?.length === 1 &&
    application.results[0].id === "mcp.conformance-initialize" &&
    application.results[0].status === "Fail" &&
    application.results[0].error?.includes(
      "MCP protocol negotiation failed.",
    ) &&
    checks.length === 0 &&
    requests?.length === 1 &&
    requests[0].method === "server/discover" &&
    requests[0].status === 200 &&
    requests[0].emptyDiscoverResult === true
  );
}

export async function startConformanceRelay(root, binding) {
  const testCase = cases.find((item) => item.id === binding.id);
  assert.ok(testCase, "Unknown conformance case");
  const url = localServerUrl(binding.url, testCase.scenario);
  await assertServerOwner(Number(url.port), binding.pid);
  const [cert, key] = await Promise.all([
    readFile(join(root, "mcp-tls/cert.pem")),
    readFile(join(root, "mcp-tls/key.pem")),
  ]);
  const sockets = new Set(),
    upstream = new Set(),
    sessions = new Set(),
    journal = [];
  const server = createServer({ cert, key }, async (req, res) => {
    let outgoing;
    try {
      assert.equal(req.url, "/mcp");
      assert.ok(["POST", "GET", "DELETE"].includes(req.method));
      assert.equal(
        req.headers.authorization,
        undefined,
        "Credentials reached unauthenticated official server",
      );
      assert.equal(req.headers.cookie, undefined);
      const chunks = [];
      let size = 0;
      for await (const chunk of req) {
        size += chunk.length;
        assert.ok(size <= 64 * 1024, "Conformance request exceeded bound");
        chunks.push(chunk);
      }
      const body = Buffer.concat(chunks);
      const message = body.length ? JSON.parse(body.toString("utf8")) : null;
      assert.ok(
        journal.length < 128,
        "Conformance relay journal exceeded bound",
      );
      const entry = {
        method: message?.method ?? req.method,
        wireVersion: req.headers["mcp-protocol-version"],
        httpMethod: req.method,
      };
      if (message?.method === "tools/call") {
        assert.equal(message.params.name, "add_numbers");
        assert.deepEqual(message.params.arguments, { a: 5, b: 3 });
        entry.argumentsVerified = true;
      }
      journal.push(entry);
      // Recheck port ownership before every exchange. No redirect, DNS lookup,
      // body/schema/metadata rewriting or synthesized protocol response.
      await assertServerOwner(Number(url.port), binding.pid);
      const headers = { ...req.headers, host: url.host };
      delete headers.connection;
      outgoing = request(
        {
          hostname: "127.0.0.1",
          port: Number(url.port),
          path: url.pathname,
          method: req.method,
          headers,
          agent: false,
        },
        (incoming) => {
          entry.status = incoming.statusCode;
          if (
            entry.method === "server/discover" &&
            incoming.headers["content-type"]?.includes("application/json")
          ) {
            // Bounded passive evidence for the raw initialize fixture's
            // incompatible probe reply. Never alter bytes/status/envelope.
            let observed = [],
              observedBytes = 0;
            incoming.on("data", (chunk) => {
              observedBytes += chunk.length;
              if (observedBytes <= 4096) observed.push(chunk);
              else observed = [];
            });
            incoming.once("end", () => {
              if (observedBytes > 4096) return;
              try {
                const value = JSON.parse(
                  Buffer.concat(observed).toString("utf8"),
                );
                entry.emptyDiscoverResult =
                  value.jsonrpc === "2.0" &&
                  value.result &&
                  typeof value.result === "object" &&
                  Object.keys(value.result).length === 0;
              } catch {
                /* No evidence from an unparseable reply. */
              }
            });
          }
          const session = incoming.headers["mcp-session-id"];
          if (session) sessions.add(session);
          if (req.method === "DELETE" && incoming.statusCode < 300)
            sessions.delete(req.headers["mcp-session-id"]);
          res.writeHead(incoming.statusCode, incoming.headers);
          incoming.on("error", () => res.destroy());
          incoming.pipe(res);
        },
      );
      upstream.add(outgoing);
      outgoing.on("close", () => upstream.delete(outgoing));
      outgoing.on("error", () => {
        entry.phase = "upstream-error";
        res.destroy();
      });
      res.once("close", () => outgoing.destroy());
      outgoing.end(body);
    } catch {
      if (journal.length < 128) journal.push({ phase: "error" });
      outgoing?.destroy();
      res.destroy();
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
      return upstream.size;
    },
    get delayedReplies() {
      return 0;
    },
    async close() {
      const pending = new Promise((resolve) => server.close(resolve));
      for (const req of upstream) req.destroy();
      for (const socket of sockets) socket.destroy();
      await pending;
      assert.equal(upstream.size, 0, "Relay retained upstream requests");
      assert.equal(sockets.size, 0);
      assert.equal(
        sessions.size,
        0,
        "Production wrapper retained official MCP session",
      );
    },
  };
}
