// External, owned OAuth issuer for the real SDK. No credential injection.
import assert from "node:assert/strict";
import { createHash, randomUUID } from "node:crypto";
import { request as httpsRequest } from "node:https";
import { request as httpRequest } from "node:http";

export const MCP_AUTH_ID = "grain-harness-auth";
export const MCP_PRIVATE_MARKER = "HARNESS_MCP_PRIVATE_";

async function boundedBody(req) {
  const chunks = [];
  let size = 0;
  for await (const chunk of req) {
    size += chunk.length;
    assert.ok(size <= 8192, "Owned OAuth request exceeded bound");
    chunks.push(chunk);
  }
  return Buffer.concat(chunks).toString("utf8");
}

export function createMcpOAuth(origin, ca, { wrongAccount = false } = {}) {
  const clients = new Map(),
    codes = new Map(),
    tokens = new Map();
  const callbacks = new Set(),
    journal = [];
  let account = "A",
    denied = false;
  const record = (item) => {
    assert.ok(journal.length < 256, "Owned MCP OAuth journal exceeded bound");
    journal.push(item);
  };
  const json = (res, status, value) => {
    res.writeHead(status, {
      "content-type": "application/json",
      "cache-control": "no-store",
    });
    res.end(JSON.stringify(value));
  };
  const metadata = () => ({
    issuer: origin(),
    authorization_endpoint: origin() + "/authorize",
    token_endpoint: origin() + "/token",
    registration_endpoint: origin() + "/register",
    response_types_supported: ["code"],
    grant_types_supported: ["authorization_code"],
    token_endpoint_auth_methods_supported: ["none"],
    code_challenge_methods_supported: ["S256"],
    scopes_supported: ["fixture.read"],
    authorization_response_iss_parameter_supported: true,
  });
  async function handle(req, res) {
    const path = new URL(req.url, origin()).pathname;
    if (["/mcp", "/account-mcp"].includes(path)) return false;
    try {
      if (
        req.method === "GET" &&
        [
          "/.well-known/oauth-protected-resource",
          "/.well-known/oauth-protected-resource/account-mcp",
        ].includes(path)
      ) {
        record({ phase: "resource-metadata" });
        json(res, 200, {
          resource: origin() + "/account-mcp",
          authorization_servers: [origin()],
          scopes_supported: ["fixture.read"],
          bearer_methods_supported: ["header"],
        });
      } else if (
        req.method === "GET" &&
        path === "/.well-known/oauth-authorization-server"
      ) {
        record({ phase: "issuer-metadata" });
        json(res, 200, metadata());
      } else if (req.method === "POST" && path === "/register") {
        const body = JSON.parse(await boundedBody(req));
        assert.equal(body.token_endpoint_auth_method, "none");
        assert.equal(body.redirect_uris.length, 1);
        const redirect = new URL(body.redirect_uris[0]);
        assert.equal(redirect.protocol, "http:");
        assert.equal(redirect.hostname, "127.0.0.1");
        assert.equal(redirect.pathname, "/mcp/oauth/callback");
        assert.ok(
          Number(redirect.port) > 0 &&
            ![7124, 17124].includes(Number(redirect.port)),
        );
        assert.equal(redirect.search, "");
        assert.equal(redirect.hash, "");
        assert.ok(clients.size < 16, "Owned registration count exceeded bound");
        const clientId = "grain-mcp-fixture-" + randomUUID();
        clients.set(clientId, redirect.href);
        record({ phase: "registered", publicClient: true });
        json(res, 201, {
          client_id: clientId,
          redirect_uris: [redirect.href],
          token_endpoint_auth_method: "none",
          grant_types: ["authorization_code"],
          response_types: ["code"],
        });
      } else if (req.method === "GET" && path === "/authorize") {
        const q = new URL(req.url, origin()).searchParams;
        const redirect = clients.get(q.get("client_id"));
        assert.ok(redirect, "Unregistered fixture client");
        assert.equal(q.get("redirect_uri"), redirect);
        assert.equal(q.get("response_type"), "code");
        assert.equal(q.get("code_challenge_method"), "S256");
        assert.match(q.get("code_challenge"), /^[\w-]{43}$/);
        assert.ok(
          q.get("state")?.length >= 20 && q.get("state").length <= 512,
          "Invalid owned OAuth state",
        );
        assert.equal(q.get("scope"), "fixture.read");
        assert.equal(q.get("resource"), origin() + "/account-mcp");
        const callback = new URL(redirect);
        callback.searchParams.set("state", q.get("state"));
        callback.searchParams.set("iss", origin());
        if (denied) callback.searchParams.set("error", "access_denied");
        else {
          assert.ok(codes.size < 16, "Owned code count exceeded bound");
          const code = randomUUID();
          codes.set(code, {
            clientId: q.get("client_id"),
            redirect,
            challenge: q.get("code_challenge"),
            account,
          });
          callback.searchParams.set("code", code);
        }
        assert.ok(callbacks.size < 16, "Owned callback count exceeded bound");
        callbacks.add(callback.href);
        record({ phase: "consent", account, denied });
        res.writeHead(302, {
          location: callback.href,
          "cache-control": "no-store",
        });
        res.end();
      } else if (req.method === "POST" && path === "/token") {
        const q = new URLSearchParams(await boundedBody(req));
        assert.equal(q.get("grant_type"), "authorization_code");
        const grant = codes.get(q.get("code"));
        assert.ok(grant, "Unissued/reused authorization code");
        assert.equal(q.get("client_id"), grant.clientId);
        assert.equal(q.get("redirect_uri"), grant.redirect);
        assert.match(q.get("code_verifier"), /^[A-Za-z0-9._~-]{43,128}$/);
        assert.equal(
          createHash("sha256")
            .update(q.get("code_verifier"))
            .digest("base64url"),
          grant.challenge,
          "PKCE verification failed",
        );
        assert.equal(q.get("resource"), origin() + "/account-mcp");
        codes.delete(q.get("code"));
        assert.ok(tokens.size < 16, "Owned token count exceeded bound");
        const token = MCP_PRIVATE_MARKER + randomUUID();
        tokens.set(token, grant.account);
        record({
          phase: "token",
          account: grant.account,
          pkceVerified: true,
          resourceVerified: true,
        });
        json(res, 200, {
          access_token: token,
          token_type: "Bearer",
          expires_in: 1200,
          scope: "fixture.read",
        });
      } else {
        record({ phase: "unexpected-route" });
        json(res, 404, { error: "Unknown owned OAuth route" });
      }
    } catch {
      // Assertions must never copy a code/token/authorization URL to evidence.
      record({ phase: "oauth-error" });
      if (!res.headersSent) json(res, 400, { error: "invalid_request" });
      else res.destroy();
    }
    return true;
  }
  function authenticatedAccount(req, res) {
    const header = req.headers.authorization;
    const selected =
      typeof header === "string" &&
      header.startsWith("Bearer ") &&
      tokens.get(header.slice(7));
    if (!selected) {
      record({ phase: "challenge" });
      res.writeHead(401, {
        "www-authenticate": `Bearer resource_metadata="${origin()}/.well-known/oauth-protected-resource/account-mcp", scope="fixture.read"`,
        "content-type": "application/json",
      });
      res.end(JSON.stringify({ error: "unauthorized" }));
      return null;
    }
    return wrongAccount ? (selected === "A" ? "B" : "A") : selected;
  }
  function exchange(url, secure) {
    return new Promise((resolve, reject) => {
      const req = (secure ? httpsRequest : httpRequest)(
        url,
        { agent: false, ...(secure ? { ca } : {}), timeout: 2500 },
        (res) => {
          let bytes = 0;
          res.on("data", (chunk) => {
            bytes += chunk.length;
            if (bytes > 1024)
              res.destroy(new Error("Owned consent response exceeded bound"));
          });
          res.on("error", reject);
          res.on("end", () =>
            resolve({ status: res.statusCode, location: res.headers.location }),
          );
        },
      );
      req.on("timeout", () =>
        req.destroy(new Error("Owned consent request timed out")),
      );
      req.on("error", (error) =>
        reject(
          Object.assign(new Error("Owned consent connection failed"), {
            code: error.code,
          }),
        ),
      );
      req.end();
    });
  }
  return {
    handle,
    authenticatedAccount,
    journal,
    configure(next) {
      assert.ok(
        Object.keys(next).every((key) => ["account", "denied"].includes(key)),
      );
      if (next.account !== undefined) {
        assert.ok(["A", "B"].includes(next.account));
        account = next.account;
      }
      if (next.denied !== undefined) {
        assert.equal(typeof next.denied, "boolean");
        denied = next.denied;
      }
    },
    async authorize(raw) {
      const url = new URL(raw);
      assert.equal(url.origin, origin());
      assert.equal(url.pathname, "/authorize");
      const reply = await exchange(url, true);
      assert.equal(
        reply.status,
        302,
        "Owned consent did not produce a callback",
      );
      assert.ok(callbacks.has(reply.location), "Unowned consent callback");
      return reply.location;
    },
    async callback(raw) {
      assert.ok(callbacks.has(raw), "Unowned callback destination");
      return exchange(new URL(raw), false);
    },
    close() {
      clients.clear();
      codes.clear();
      tokens.clear();
      callbacks.clear();
    },
  };
}
