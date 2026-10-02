// External provider simulation only; Grain still owns PKCE, callbacks and vault.
import assert from "node:assert/strict";
import { createServer, request as httpsRequest } from "node:https";
import { createHash, randomUUID } from "node:crypto";
import { readFile } from "node:fs/promises";
import { join } from "node:path";

export const AUTH_FIXTURE_ID = "com.grain.harness.auth";
export const PRIVATE_MARKER = "HARNESS_OAUTH_PRIVATE_";

export async function startAuthFixture(root, { wrongAccount = false } = {}) {
  const cert = await readFile(join(root, "auth-tls/cert.pem"));
  const key = await readFile(join(root, "auth-tls/key.pem"));
  const codes = new Map(),
    tokens = new Map(),
    refreshTokens = new Map();
  const journal = [],
    sockets = new Set(),
    held = new Set();
  let mode = {
    account: "A",
    deny: false,
    partial: false,
    expiresIn: 1200,
    holdToken: false,
    clientId: "grain-harness-public",
    scope: "fixture.read",
    tokenPath: "/token",
  };
  function record(entry) {
    assert.ok(journal.length < 256, "OAuth fixture journal overflow");
    journal.push(entry);
  }
  const json = (res, status, value) => {
    res.writeHead(status, {
      "content-type": "application/json",
      "cache-control": "no-store",
    });
    res.end(JSON.stringify(value));
  };
  const server = createServer({ key, cert }, async (req, res) => {
    try {
      const url = new URL(req.url, "https://127.0.0.1");
      if (req.method === "GET" && url.pathname === "/authorize") {
        const query = url.searchParams;
        assert.ok(
          ["grain-harness-public", "grain-harness-public-v2"].includes(
            query.get("client_id"),
          ),
        );
        assert.equal(query.get("response_type"), "code");
        assert.equal(query.get("client_id"), mode.clientId);
        assert.equal(query.get("code_challenge_method"), "S256");
        assert.ok(
          ["fixture.read", "fixture.read fixture.extra"].includes(
            query.get("scope"),
          ),
        );
        assert.match(query.get("code_challenge"), /^[\w-]{43}$/);
        assert.equal(query.get("scope"), mode.scope);
        assert.match(query.get("state"), /^[a-f\d]{64}$/);
        const redirect = new URL(query.get("redirect_uri"));
        assert.equal(redirect.protocol, "http:");
        assert.equal(redirect.hostname, "127.0.0.1");
        assert.ok(Number(redirect.port) > 0);
        assert.match(redirect.pathname, /^\/grain\/oauth\/[a-f\d-]{36}$/);
        assert.equal(redirect.search, "");
        redirect.searchParams.set("state", query.get("state"));
        record({ phase: "consent", account: mode.account, denied: mode.deny });
        if (mode.deny) redirect.searchParams.set("error", "access_denied");
        else {
          assert.ok(codes.size < 32, "Authorization code bound");
          const code = randomUUID();
          codes.set(code, {
            ...mode,
            challenge: query.get("code_challenge"),
            redirect: query.get("redirect_uri"),
            clientId: query.get("client_id"),
            scope: query.get("scope"),
          });
          redirect.searchParams.set("code", code);
        }
        res.writeHead(302, {
          location: redirect.href,
          "cache-control": "no-store",
        });
        res.end();
      } else if (
        req.method === "POST" &&
        ["/token", "/token-v2"].includes(url.pathname)
      ) {
        let body = "";
        for await (const chunk of req) {
          body += chunk.toString();
          assert.ok(body.length <= 16384, "Token request bound");
        }
        const params = new URLSearchParams(body);
        const grant = params.get("grant_type");
        let issued;
        if (grant === "authorization_code") {
          issued = codes.get(params.get("code"));
          assert.ok(issued, "Unknown or consumed code");
          codes.delete(params.get("code"));
          assert.equal(params.get("redirect_uri"), issued.redirect);
          assert.equal(
            createHash("sha256")
              .update(params.get("code_verifier") ?? "")
              .digest("base64url"),
            issued.challenge,
            "PKCE failed",
          );
        } else {
          assert.equal(grant, "refresh_token");
          issued = refreshTokens.get(params.get("refresh_token"));
          assert.ok(issued, "Unknown refresh token");
        }
        assert.equal(params.get("client_id"), issued.clientId);
        assert.equal(url.pathname, issued.tokenPath);
        record({
          phase: "exchange",
          account: issued.account,
          grant,
          pkceVerified: grant === "authorization_code",
          endpoint: url.pathname,
        });
        if (issued.holdToken)
          await new Promise((resolve) => {
            held.add(resolve);
            res.once("close", () => {
              held.delete(resolve);
              resolve();
            });
          });
        if (res.destroyed) return;
        assert.ok(tokens.size < 128, "Token fixture bound");
        const access = PRIVATE_MARKER + randomUUID(),
          refresh = PRIVATE_MARKER + randomUUID();
        tokens.set(access, issued);
        refreshTokens.set(refresh, issued);
        json(res, 200, {
          access_token: access,
          refresh_token: refresh,
          token_type: "Bearer",
          expires_in: issued.expiresIn,
          scope: issued.partial ? "fixture.other" : issued.scope,
        });
      } else if (req.method === "GET" && url.pathname === "/me") {
        const issued = tokens.get(
          req.headers.authorization?.slice("Bearer ".length),
        );
        if (!issued) {
          json(res, 401, { error: "invalid_grant" });
          return;
        }
        record({ phase: "read", account: issued.account });
        json(res, 200, {
          account: wrongAccount
            ? issued.account === "A"
              ? "B"
              : "A"
            : issued.account,
        });
      } else json(res, 404, { error: "not_found" });
    } catch {
      record({ phase: "rejected" });
      if (!res.headersSent) json(res, 400, { error: "invalid_request" });
      else res.destroy();
    }
  });
  server.on("connection", (socket) => {
    sockets.add(socket);
    socket.once("close", () => sockets.delete(socket));
  });
  server.headersTimeout = 5000;
  server.requestTimeout = 10000;
  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolve);
  });
  const port = server.address().port;
  return {
    port,
    journal,
    configure(update) {
      mode = { ...mode, ...update };
      assert.ok(["A", "B"].includes(mode.account));
    },
    release() {
      for (const resolve of held) resolve();
      held.clear();
    },
    async authorize(raw) {
      const url = new URL(raw);
      assert.equal(url.origin, `https://127.0.0.1:${port}`);
      assert.equal(url.pathname, "/authorize");
      const location = await new Promise((resolve, reject) => {
        const req = httpsRequest(url, { ca: cert, agent: false }, (res) => {
          res.resume();
          try {
            assert.equal(res.statusCode, 302);
            resolve(res.headers.location);
          } catch (error) {
            reject(error);
          }
        });
        req.setTimeout(5000, () =>
          req.destroy(new Error("Consent request timeout")),
        );
        req.once("error", reject);
        req.end();
      });
      const callback = new URL(location);
      assert.equal(
        callback.origin,
        new URL(url.searchParams.get("redirect_uri")).origin,
      );
      assert.equal(
        callback.pathname,
        new URL(url.searchParams.get("redirect_uri")).pathname,
      );
      assert.equal(
        callback.searchParams.get("state"),
        url.searchParams.get("state"),
      );
      const response = await fetch(callback, {
        redirect: "error",
        signal: AbortSignal.timeout(5000),
      });
      await response.text();
      assert.equal(response.status, mode.deny ? 400 : 200);
    },
    async close() {
      for (const resolve of held) resolve();
      held.clear();
      await new Promise((resolve, reject) => {
        server.close((error) => (error ? reject(error) : resolve()));
        server.closeAllConnections();
        for (const socket of sockets) socket.destroy();
      });
      codes.clear();
      tokens.clear();
      refreshTokens.clear();
      assert.equal(sockets.size, 0);
    },
  };
}

export function authPackage(port, { owner, change } = {}) {
  assert.ok(Number.isInteger(port) && port > 0 && port <= 65535);
  assert.ok(
    !owner || ["installed", "developer-a", "developer-b"].includes(owner),
  );
  assert.ok(!change || ["client", "token", "scopes", "hosts"].includes(change));
  const pack = {
    manifest: {
      id: AUTH_FIXTURE_ID,
      name: "Harness Native Account",
      version: "0.1.0",
      grainApi: "^1.0",
      tier: "scripted",
      kind: "extending",
      permissions: ["auth", "net:127.0.0.1"],
      entry_source: `grain.actions({ account_read: async () => { const reply = await grain.net.fetch("https://127.0.0.1:${port}/me", {auth: true}); if (!reply.ok) return {error: {message: "Fixture read failed"}}; const actual = JSON.parse(reply.body); ${owner ? `actual.owner = ${JSON.stringify(owner)};` : ""} return {ok: {body: "Harness account reply: " + JSON.stringify(actual)}}; } });`,
      contributes: {
        authentication: {
          type: "oauth2-pkce",
          providerName: "Harness OAuth",
          clientId: "grain-harness-public",
          authorizationEndpoint: `https://127.0.0.1:${port}/authorize`,
          tokenEndpoint: `https://127.0.0.1:${port}/token`,
          scopes: ["fixture.read"],
          apiHosts: ["127.0.0.1"],
          redirectMethods: ["loopback"],
        },
        actions: [
          {
            id: "account_read",
            title: "Harness account read",
            risk: "confirm",
            when: {},
            utterances: ["read harness account"],
          },
        ],
      },
    },
    payloads: {},
  };
  const declaration = pack.manifest.contributes.authentication;
  if (change === "client") declaration.clientId += "-v2";
  if (change === "token") declaration.tokenEndpoint += "-v2";
  if (change === "scopes") declaration.scopes.push("fixture.extra");
  if (change === "hosts") {
    declaration.apiHosts.push("localhost");
    pack.manifest.permissions.push("net:localhost");
  }
  return pack;
}
