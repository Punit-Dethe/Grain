// External, owned OAuth issuer for the real SDK. No credential injection.
import assert from "node:assert/strict";
import { createHash, randomUUID } from "node:crypto";
import { request as httpsRequest } from "node:https";
import { request as httpRequest } from "node:http";

export const MCP_AUTH_ID = "grain-harness-auth";
export const MCP_CLIENT_ID = "grain-harness-auth-client";
export const MCP_PEER_ID = "grain-harness-auth-peer";
export const MCP_PEER_CLIENT_ID = "grain-harness-auth-peer-client";
export const MCP_CLIENTS = Object.freeze({
  publicOne: "grain-mcp-public-one",
  publicTwo: "grain-mcp-public-two",
  confidential: "grain-mcp-confidential",
});
export const MCP_PRIVATE_MARKER = "HARNESS_MCP_PRIVATE_";
export const MCP_CLIENT_SECRETS = Object.freeze([
  MCP_PRIVATE_MARKER + "owned-client-secret-one",
  MCP_PRIVATE_MARKER + "owned-client-secret-two",
]);
// Ten combined auth cases legitimately exhaust the former 1024-entry log.
// Keep complete evidence bounded, with one reserved terminal error slot.
export const MCP_OAUTH_JOURNAL_LIMIT = 2048;

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
  const fixedRedirect = "http://127.0.0.1:31938/mcp/oauth/callback";
  const clients = new Map([
      [MCP_CLIENTS.publicOne, { redirect: fixedRedirect }],
      [MCP_CLIENTS.publicTwo, { redirect: fixedRedirect }],
      [
        MCP_CLIENTS.confidential,
        { redirect: fixedRedirect, confidential: true },
      ],
    ]),
    codes = new Map(),
    tokens = new Map(),
    refreshTokens = new Map();
  const callbacks = new Set(),
    journal = [];
  let account = "A",
    denied = false,
    secretVersion = 0,
    expiresIn = 1200,
    refreshable = false,
    refreshExpiresIn = 1200,
    rejectRefresh = false,
    refreshFailure = "none",
    wrongRefreshAccount = false,
    authorizationServer = null,
    resourceOrigin = null,
    metadataUnavailable = false,
    metadataClientSupported = false,
    dynamicRegistration = true,
    clientDocument = "valid",
    destinationMode = "valid";
  const record = (item) => {
    assert.ok(
      journal.length < MCP_OAUTH_JOURNAL_LIMIT - 1,
      "Owned MCP OAuth journal exceeded bound",
    );
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
    authorization_endpoint:
      destinationMode === "private-authorization"
        ? "https://169.254.169.254/authorize"
        : origin() + "/authorize",
    token_endpoint:
      destinationMode === "other-token-port"
        ? "https://127.0.0.1:1/token"
        : destinationMode === "remote-token"
          ? "https://unowned.example.com/token"
          : origin() + "/token",
    ...(destinationMode === "oversized-metadata"
      ? { padding: "x".repeat(1024 * 1024) }
      : {}),
    ...(dynamicRegistration
      ? { registration_endpoint: origin() + "/register" }
      : {}),
    ...(metadataClientSupported
      ? { client_id_metadata_document_supported: true }
      : {}),
    response_types_supported: ["code"],
    grant_types_supported: [
      "authorization_code",
      ...(refreshable ? ["refresh_token"] : []),
    ],
    token_endpoint_auth_methods_supported: ["none", "client_secret_post"],
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
          authorization_servers: [authorizationServer ?? origin()],
          scopes_supported: ["fixture.read"],
          bearer_methods_supported: ["header"],
        });
      } else if (
        req.method === "GET" &&
        path === "/.well-known/oauth-authorization-server"
      ) {
        record({ phase: "issuer-metadata" });
        if (metadataUnavailable)
          json(res, 503, { error: "fixture_metadata_unavailable" });
        else json(res, 200, metadata());
      } else if (req.method === "GET" && path === "/oauth/client.json") {
        record({ phase: "client-document", mode: clientDocument });
        if (clientDocument === "unavailable")
          json(res, 503, { error: "unavailable" });
        else
          json(res, 200, {
            client_id:
              origin() +
              (clientDocument === "wrong-id"
                ? "/wrong-client.json"
                : "/oauth/client.json"),
            client_name: "Grain Agent Harness",
            redirect_uris: [
              clientDocument === "wrong-redirect"
                ? "http://127.0.0.1:31939/mcp/oauth/callback"
                : fixedRedirect,
            ],
            application_type: "native",
            grant_types: ["authorization_code", "refresh_token"],
            response_types: ["code"],
            token_endpoint_auth_method: "none",
          });
      } else if (req.method === "POST" && path === "/register") {
        assert.equal(
          dynamicRegistration,
          true,
          "Unadvertised DCR was attempted",
        );
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
        assert.ok(clients.size < 32, "Owned registration count exceeded bound");
        const clientId = "grain-mcp-fixture-" + randomUUID();
        clients.set(clientId, { redirect: redirect.href });
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
        if (q.get("client_id") === origin() + "/oauth/client.json") {
          assert.equal(metadataClientSupported, true);
          // The authorization server fetches the document, not Grain. Use the
          // actual owned HTTPS endpoint and scoped CA, not an in-memory shortcut.
          const fetched = await fetchClientDocument();
          const document = fetched.value;
          if (clientDocument === "unavailable") {
            assert.equal(fetched.status, 503);
            assert.deepEqual(document, { error: "unavailable" });
          } else {
            assert.equal(fetched.status, 200);
            assert.equal(
              document.client_id,
              origin() +
                (clientDocument === "wrong-id"
                  ? "/wrong-client.json"
                  : "/oauth/client.json"),
            );
            assert.equal(document.application_type, "native");
            assert.equal(document.token_endpoint_auth_method, "none");
            assert.deepEqual(document.grant_types, [
              "authorization_code",
              "refresh_token",
            ]);
            assert.deepEqual(document.response_types, ["code"]);
            assert.deepEqual(document.redirect_uris, [
              clientDocument === "wrong-redirect"
                ? "http://127.0.0.1:31939/mcp/oauth/callback"
                : fixedRedirect,
            ]);
            assert.equal(q.get("redirect_uri"), fixedRedirect);
          }
          // Confirm the exact injected fault arrived over HTTPS before counting
          // its refusal. A dropped connection/other fixture error cannot pass it.
          if (clientDocument !== "valid") {
            record({ phase: "client-document-refused", mode: clientDocument });
            json(res, 400, { error: "invalid_client_metadata" });
            return true;
          }
          clients.set(q.get("client_id"), {
            redirect: fixedRedirect,
            metadata: true,
          });
          record({
            phase: "metadata-client-accepted",
            fixedRedirect: true,
            publicClient: true,
          });
        }
        const client = clients.get(q.get("client_id"));
        assert.ok(client, "Unregistered fixture client");
        const redirect = client.redirect;
        assert.equal(q.get("redirect_uri"), redirect);
        assert.equal(q.get("response_type"), "code");
        assert.equal(q.get("code_challenge_method"), "S256");
        assert.match(q.get("code_challenge"), /^[\w-]{43}$/);
        assert.ok(
          q.get("state")?.length >= 20 && q.get("state").length <= 512,
          "Invalid owned OAuth state",
        );
        assert.equal(q.get("scope"), "fixture.read");
        assert.equal(
          q.get("resource"),
          (resourceOrigin ?? origin()) + "/account-mcp",
        );
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
        record({
          phase: "consent",
          account,
          denied,
          metadataClient: !!client.metadata,
        });
        res.writeHead(302, {
          location: callback.href,
          "cache-control": "no-store",
        });
        res.end();
      } else if (req.method === "POST" && path === "/token") {
        const q = new URLSearchParams(await boundedBody(req));
        record({
          phase: "token-attempt",
          secretPresent: q.has("client_secret"),
          secretVersion: q.has("client_secret")
            ? MCP_CLIENT_SECRETS.indexOf(q.get("client_secret"))
            : null,
        });
        if (q.get("grant_type") === "refresh_token") {
          if (refreshFailure !== "none") {
            record({ phase: "refresh-unavailable", mode: refreshFailure });
            if (refreshFailure === "server")
              json(res, 503, {
                error: "temporarily_unavailable",
                error_description: MCP_PRIVATE_MARKER,
              });
            else if (refreshFailure === "malformed") {
              res.writeHead(200, { "content-type": "application/json" });
              res.end('{"access_token":');
            } else req.socket.destroy();
            return true;
          }
          const old = refreshTokens.get(q.get("refresh_token"));
          if (!old || rejectRefresh) {
            record({ phase: "refresh-refused", reason: "invalid_grant" });
            json(res, 400, { error: "invalid_grant" });
            return true;
          }
          assert.equal(q.get("client_id"), old.clientId);
          assert.equal(
            q.get("resource"),
            (resourceOrigin ?? origin()) + "/account-mcp",
          );
          assert.equal(q.get("client_secret"), old.secret);
          refreshTokens.delete(q.get("refresh_token"));
          const selected = wrongRefreshAccount
            ? old.account === "A"
              ? "B"
              : "A"
            : old.account;
          const issued = issue(
            selected,
            old.clientId,
            old.secret,
            refreshExpiresIn,
            true,
          );
          record({
            phase: "refresh",
            account: selected,
            generation: old.generation + 1,
            afterExpiry: Date.now() >= old.expiresAt,
            resourceVerified: true,
            rotated: true,
            issuedAt: issued.issuedAt,
            expiresAt: issued.expiresAt,
            expiresIn: refreshExpiresIn,
          });
          refreshTokens.get(issued.reply.refresh_token).generation =
            old.generation + 1;
          json(res, 200, issued.reply);
          return true;
        }
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
        assert.equal(
          q.get("resource"),
          (resourceOrigin ?? origin()) + "/account-mcp",
        );
        codes.delete(q.get("code"));
        const client = clients.get(grant.clientId);
        assert.ok(client, "Unknown grant client");
        const expectedSecret = client.confidential
          ? MCP_CLIENT_SECRETS[secretVersion]
          : null;
        if (q.get("client_secret") !== expectedSecret) {
          record({
            phase: "client-auth-refused",
            confidential: !!client.confidential,
          });
          json(res, 400, { error: "invalid_client" });
          return true;
        }
        const issued = issue(
          grant.account,
          grant.clientId,
          expectedSecret,
          expiresIn,
          refreshable,
        );
        record({
          phase: "token",
          account: grant.account,
          pkceVerified: true,
          resourceVerified: true,
          registration: client.confidential ? "confidential" : "public",
          configuredClient: Object.values(MCP_CLIENTS).includes(grant.clientId),
          metadataClient: !!client.metadata,
          issuedAt: issued.issuedAt,
          expiresAt: issued.expiresAt,
          expiresIn,
          refreshable,
          ...(client.confidential ? { secretVersion } : {}),
        });
        json(res, 200, issued.reply);
      } else {
        record({ phase: "unexpected-route" });
        json(res, 404, { error: "Unknown owned OAuth route" });
      }
    } catch {
      // Assertions must never copy a code/token/authorization URL to evidence.
      if (journal.length < MCP_OAUTH_JOURNAL_LIMIT)
        journal.push({ phase: "oauth-error" });
      if (!res.headersSent) json(res, 400, { error: "invalid_request" });
      else res.destroy();
    }
    return true;
  }
  function issue(selected, clientId, secret, lifetime, withRefresh) {
    assert.ok(tokens.size < 32, "Owned token count exceeded bound");
    assert.ok(refreshTokens.size < 32, "Owned refresh count exceeded bound");
    const token = MCP_PRIVATE_MARKER + randomUUID();
    const issuedAt = Date.now(),
      expiresAt = issuedAt + lifetime * 1000;
    tokens.set(token, { account: selected, expiresAt });
    const reply = {
      access_token: token,
      token_type: "Bearer",
      expires_in: lifetime,
      scope: "fixture.read",
    };
    if (withRefresh) {
      reply.refresh_token = MCP_PRIVATE_MARKER + randomUUID();
      refreshTokens.set(reply.refresh_token, {
        account: selected,
        clientId,
        secret,
        expiresAt,
        generation: 0,
      });
    }
    return { reply, issuedAt, expiresAt };
  }
  function fetchClientDocument() {
    return new Promise((resolve, reject) => {
      const req = httpsRequest(
        origin() + "/oauth/client.json",
        { agent: false, ca, timeout: 2500 },
        (res) => {
          const chunks = [];
          let bytes = 0;
          res.on("data", (chunk) => {
            bytes += chunk.length;
            if (bytes > 2048)
              res.destroy(new Error("Owned client document exceeded bound"));
            else chunks.push(chunk);
          });
          res.on("error", reject);
          res.on("end", () => {
            try {
              assert.equal(res.headers["content-type"], "application/json");
              resolve({
                status: res.statusCode,
                value: JSON.parse(Buffer.concat(chunks).toString("utf8")),
              });
            } catch {
              reject(new Error("Owned client document unavailable or invalid"));
            }
          });
        },
      );
      req.on("timeout", () =>
        req.destroy(new Error("Owned client document timed out")),
      );
      req.on("error", () =>
        reject(new Error("Owned client document connection failed")),
      );
      req.end();
    });
  }
  function authenticatedAccount(req, res) {
    const header = req.headers.authorization;
    const selected =
      typeof header === "string" &&
      header.startsWith("Bearer ") &&
      tokens.get(header.slice(7));
    if (!selected || Date.now() >= selected.expiresAt) {
      record({ phase: "challenge", ...(selected ? { expired: true } : {}) });
      res.writeHead(401, {
        "www-authenticate": `Bearer resource_metadata="${origin()}/.well-known/oauth-protected-resource/account-mcp", scope="fixture.read"`,
        "content-type": "application/json",
      });
      res.end(JSON.stringify({ error: "unauthorized" }));
      return null;
    }
    return wrongAccount
      ? selected.account === "A"
        ? "B"
        : "A"
      : selected.account;
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
    retireGrants() {
      // Call only after host logout/zero inventory. Receipt history remains;
      // the fixture must not keep completed scenarios' bearer grants alive.
      tokens.clear();
      refreshTokens.clear();
    },
    retireRegistrations() {
      // Explicit scenario boundary only, after host logout and zero inventory.
      // Never discard an active grant or a pending consent delivery.
      assert.equal(
        tokens.size + refreshTokens.size + callbacks.size,
        0,
        "Owned registrations still have active grants or consent",
      );
      const fixed = new Set(Object.values(MCP_CLIENTS));
      for (const id of clients.keys()) if (!fixed.has(id)) clients.delete(id);
      codes.clear();
    },
    configure(next) {
      assert.ok(
        Object.keys(next).every((key) =>
          [
            "account",
            "denied",
            "secretVersion",
            "expiresIn",
            "refreshable",
            "refreshExpiresIn",
            "rejectRefresh",
            "refreshFailure",
            "wrongRefreshAccount",
            "authorizationServer",
            "resourceOrigin",
            "metadataUnavailable",
            "metadataClientSupported",
            "dynamicRegistration",
            "clientDocument",
            "destinationMode",
          ].includes(key),
        ),
      );
      if (next.destinationMode !== undefined) {
        assert.ok(
          [
            "valid",
            "other-token-port",
            "remote-token",
            "private-authorization",
            "oversized-metadata",
          ].includes(next.destinationMode),
        );
        destinationMode = next.destinationMode;
      }
      for (const key of ["authorizationServer", "resourceOrigin"]) {
        if (next[key] !== undefined && next[key] !== null) {
          const value = new URL(next[key]);
          assert.equal(value.protocol, "https:");
          assert.equal(value.hostname, "127.0.0.1");
          assert.ok(Number(value.port) > 0);
          assert.equal(value.origin, next[key]);
        }
      }
      if (next.authorizationServer !== undefined)
        authorizationServer = next.authorizationServer;
      if (next.resourceOrigin !== undefined)
        resourceOrigin = next.resourceOrigin;
      if (next.metadataUnavailable !== undefined) {
        assert.equal(typeof next.metadataUnavailable, "boolean");
        metadataUnavailable = next.metadataUnavailable;
      }
      for (const key of ["metadataClientSupported", "dynamicRegistration"]) {
        if (next[key] !== undefined) assert.equal(typeof next[key], "boolean");
      }
      if (next.metadataClientSupported !== undefined)
        metadataClientSupported = next.metadataClientSupported;
      if (next.dynamicRegistration !== undefined)
        dynamicRegistration = next.dynamicRegistration;
      if (next.clientDocument !== undefined) {
        assert.ok(
          ["valid", "wrong-id", "wrong-redirect", "unavailable"].includes(
            next.clientDocument,
          ),
        );
        clientDocument = next.clientDocument;
      }
      if (next.account !== undefined) {
        assert.ok(["A", "B"].includes(next.account));
        account = next.account;
      }
      if (next.denied !== undefined) {
        assert.equal(typeof next.denied, "boolean");
        denied = next.denied;
      }
      if (next.secretVersion !== undefined) {
        assert.ok([0, 1].includes(next.secretVersion));
        secretVersion = next.secretVersion;
      }
      for (const key of ["expiresIn", "refreshExpiresIn"]) {
        if (next[key] !== undefined)
          assert.ok(
            Number.isInteger(next[key]) && next[key] >= 1 && next[key] <= 1200,
          );
      }
      for (const key of [
        "refreshable",
        "rejectRefresh",
        "wrongRefreshAccount",
      ]) {
        if (next[key] !== undefined) assert.equal(typeof next[key], "boolean");
      }
      if (next.expiresIn !== undefined) expiresIn = next.expiresIn;
      if (next.refreshExpiresIn !== undefined)
        refreshExpiresIn = next.refreshExpiresIn;
      if (next.refreshable !== undefined) refreshable = next.refreshable;
      if (next.rejectRefresh !== undefined) rejectRefresh = next.rejectRefresh;
      if (next.refreshFailure !== undefined) {
        assert.ok(
          ["none", "server", "malformed", "dropped"].includes(
            next.refreshFailure,
          ),
        );
        refreshFailure = next.refreshFailure;
      }
      if (next.wrongRefreshAccount !== undefined)
        wrongRefreshAccount = next.wrongRefreshAccount;
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
      // Ownership is needed only for this one delivery, including a refused
      // late callback. Keep its issuer code alive so Grain must reject it.
      assert.ok(callbacks.delete(raw), "Unowned/consumed callback destination");
      return exchange(new URL(raw), false);
    },
    close() {
      clients.clear();
      codes.clear();
      tokens.clear();
      refreshTokens.clear();
      callbacks.clear();
    },
  };
}
