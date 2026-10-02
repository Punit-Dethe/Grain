// Actual SDK/DCR consent creation and cancellation only. Never opens a browser,
// submits a code, imports a grant, calls a tool or reports the private consent URL.
import assert from "node:assert/strict";
import { createServer, connect } from "node:net";
export const LINEAR_ID = "grain-harness-linear";
export const LINEAR_ENDPOINT = "https://mcp.linear.app/mcp/readonly";

export function validateLinearConsent(raw) {
  // Generic errors intentionally exclude the raw URL, state and client ID.
  const fail = () => {
    throw new Error("Invalid read-only Linear SDK consent");
  };
  let url;
  try {
    url = new URL(raw);
  } catch {
    return fail();
  }
  const query = url.searchParams;
  if (
    typeof raw !== "string" ||
    raw.length > 8192 ||
    url.origin !== "https://mcp.linear.app" ||
    url.pathname !== "/authorize" ||
    url.username ||
    url.password ||
    url.hash ||
    [...query].length !== 8 ||
    new Set(query.keys()).size !== 8 ||
    query.get("scope") !== "read" ||
    query.get("resource") !== LINEAR_ENDPOINT ||
    query.get("response_type") !== "code" ||
    query.get("code_challenge_method") !== "S256" ||
    !/^[A-Za-z0-9_-]{43}$/.test(query.get("code_challenge") ?? "") ||
    !query.get("state") ||
    query.get("state").length < 16 ||
    query.get("state").length > 4096 ||
    !query.get("client_id") ||
    query.get("client_id").length > 4096
  )
    return fail();
  let callback;
  try {
    callback = new URL(query.get("redirect_uri"));
  } catch {
    return fail();
  }
  if (
    callback.protocol !== "http:" ||
    callback.hostname !== "127.0.0.1" ||
    !callback.port ||
    ["0", "7124", "17124"].includes(callback.port) ||
    callback.pathname !== "/mcp/oauth/callback" ||
    callback.username ||
    callback.password ||
    callback.hash ||
    callback.search
  )
    return fail();
  return {
    callbackPort: Number(callback.port),
    scope: "read",
    resource: LINEAR_ENDPOINT,
    pkce: "S256",
  };
}

export function verifyLinearCancellation(outcome) {
  // Accept the precise production cancellation, never a registration/network error.
  if (outcome?.error !== "MCP account or access changed. Ask again.")
    throw new Error(
      "Linear connect did not end with the expected host cancellation",
    );
}

async function reusable(port) {
  const server = createServer();
  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(port, "127.0.0.1", resolve);
  });
  await new Promise((resolve, reject) =>
    server.close((error) => (error ? reject(error) : resolve())),
  );
}
async function portClosed(port) {
  return new Promise((resolve) => {
    const socket = connect({ host: "127.0.0.1", port });
    const finish = (value) => {
      socket.destroy();
      resolve(value);
    };
    socket.once("connect", () => finish(false));
    socket.once("error", (error) => finish(error.code === "ECONNREFUSED"));
    socket.setTimeout(1000, () => finish(false));
  });
}

export function linearHandlers(ctx) {
  let evidence = [];
  const control = (operation) =>
    ctx.invoke("agent_harness_mcp", { operation, target: "linear" });
  async function disconnected() {
    const state = (await ctx.invoke("mcp_provider_status")).find(
      (x) => x.id === LINEAR_ID,
    );
    assert.ok(state, "Isolated Linear catalog identity missing");
    assert.equal(state.endpoint, LINEAR_ENDPOINT);
    assert.equal(state.connected, false);
    assert.equal(state.enabled, false);
    assert.equal(state.state, "disconnected");
    assert.equal(await ctx.vaultCount(), 0);
  }
  async function cancellation(operation) {
    await control("disconnect");
    await disconnected();
    for (const stage of ["initial", "fresh-retry"]) {
      let settled;
      const pending = control("connect").then(
        (value) => (settled = { value }),
        // Drop page.evaluate's wrapper without retaining SDK private errors.
        (error) =>
          (settled = {
            error: String(error.message ?? error).replace(
              /^page\.evaluate: /,
              "",
            ),
          }),
      );
      let metadata;
      try {
        const raw = await ctx.waitFor(
          "Actual SDK Linear consent handoff",
          async () => {
            const value = await control("authorization");
            if (value) return value;
            if (settled)
              throw new Error(
                "Linear SDK consent creation failed before handoff",
              );
          },
          { timeoutMs: 60000 },
        );
        metadata = validateLinearConsent(raw);
        assert.equal(await control("authorization"), null);
        if (!(ctx.fault === "skip-linear-cancel" && stage === "initial"))
          await control(operation);
        const outcome = await ctx.waitFor(
          "Linear connect cancellation settles",
          async () => settled,
          { timeoutMs: 5000 },
        );
        verifyLinearCancellation(outcome);
        await pending;
        assert.equal(await control("authorization"), null);
        await reusable(metadata.callbackPort);
        assert.equal(
          await portClosed(metadata.callbackPort),
          true,
          "Cancelled Linear callback listener survived",
        );
        await disconnected();
        evidence.push({
          stage,
          operation,
          status: "Pass",
          scope: metadata.scope,
          resource: metadata.resource,
          pkce: metadata.pkce,
          actualSdkConsent: true,
          callbackReleased: true,
          lateCallbackConnectionRefused: true,
          vaultEntries: 0,
          browserOpened: false,
          credentialExchangeRequestedByRunner: false,
          toolExecutionPermitted: false,
        });
      } finally {
        await control("disconnect");
        await ctx.waitFor(
          "Linear preflight owned connect retirement",
          async () => settled,
          { timeoutMs: 5000 },
        );
        await pending;
        if (metadata) await reusable(metadata.callbackPort);
      }
      if (stage === "initial") {
        await ctx.restartHost();
        await disconnected();
      }
    }
  }
  return {
    handlers: {
      "mcp.linear-cancel-disable": () => cancellation("disable"),
      "mcp.linear-cancel-disconnect": () => cancellation("disconnect"),
    },
    takeEvidence() {
      const result = evidence;
      evidence = [];
      return result;
    },
  };
}
