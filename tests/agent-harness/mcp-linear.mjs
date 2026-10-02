// Actual SDK/DCR. Browser consent requires an explicit separate sign-in mode;
// no code/grant import, tool execution or private content in retained evidence.
import assert from "node:assert/strict";
import { createServer, connect } from "node:net";
import { createInterface } from "node:readline";
export const LINEAR_ID = "grain-harness-linear";
export const LINEAR_ENDPOINT = "https://mcp.linear.app/mcp/readonly";

export function linearIpcDeadline(command, args, signIn) {
  if (signIn && command === "agent_harness_mcp" && args.target === "linear") {
    if (args.operation === "connect") return 310000;
    if (args.operation === "discover") return 95000;
  }
  return 10000;
}

export function safeLinearError(error) {
  const message = String(error?.message ?? error);
  // Allow fixed application diagnostics only; SDK/provider/assertion bodies can
  // contain tokens or workspace data and are intentionally never retained.
  for (const text of [
    "Linear test grant is not a verifiable read-only grant",
    "Linear browser cancellation unexpectedly authorized the account",
    "Linear SDK consent creation failed before handoff",
    "Linear browser sign-in did not complete",
    "Linear grant metadata failed validation",
    "Linear read-only discovery failed",
    "Linear cancellation requires typing only cancelled",
    "Linear cancellation acknowledgement timed out",
    "Linear cancellation acknowledgement was interrupted",
    "Linear cancellation acknowledgement input closed",
  ])
    if (message === text) return text;
  if (message.startsWith("Owned MCP credential cleanup:"))
    return "Owned Linear credential cleanup failed";
  return "Linear test failed; private diagnostics were omitted";
}

export function verifyLinearGrantMetadata(value) {
  const fail = () => {
    throw new Error("Linear grant metadata failed validation");
  };
  const keys = [
    "connected",
    "scope",
    "issuerVerified",
    "scopeSource",
    "issuedAtEpochSeconds",
    "expiresInSeconds",
    "expiresAtEpochSeconds",
    "expiryKnown",
    "expired",
    "refreshAvailable",
  ];
  if (
    !value ||
    typeof value !== "object" ||
    Object.keys(value).length !== keys.length ||
    Object.keys(value).some((key) => !keys.includes(key)) ||
    value.connected !== true ||
    value.scope !== "read" ||
    value.issuerVerified !== true ||
    !["token_response", "sdk_requested_scope_rfc6749"].includes(
      value.scopeSource,
    ) ||
    !Number.isSafeInteger(value.issuedAtEpochSeconds) ||
    value.issuedAtEpochSeconds <= 0 ||
    typeof value.refreshAvailable !== "boolean" ||
    typeof value.expiryKnown !== "boolean"
  )
    return fail();
  if (value.expiryKnown) {
    if (
      !Number.isSafeInteger(value.expiresInSeconds) ||
      value.expiresInSeconds < 0 ||
      !Number.isSafeInteger(value.expiresAtEpochSeconds) ||
      value.expiresAtEpochSeconds !==
        value.issuedAtEpochSeconds + value.expiresInSeconds ||
      value.expired !== false
    )
      return fail();
  } else if (
    value.expiresInSeconds !== null ||
    value.expiresAtEpochSeconds !== null ||
    value.expired !== null
  )
    return fail();
  return Object.fromEntries(keys.map((key) => [key, value[key]]));
}

export function verifyLinearDiscoveryCounts(value) {
  if (
    !value ||
    Object.keys(value).length !== 1 ||
    !Number.isSafeInteger(value.tool_count) ||
    value.tool_count <= 0 ||
    value.tool_count > 128
  )
    throw new Error("Linear read-only discovery failed");
  return {
    supportedToolCount: value.tool_count,
    toolExecutionPermitted: false,
  };
}

function consentOutcome(error) {
  const message = String(error?.message ?? error).replace(
    /^page\.evaluate: /,
    "",
  );
  if (message === "provider denied authentication") return { kind: "denied" };
  if (message === "MCP account or access changed. Ask again.")
    return { kind: "cancelled" };
  return { kind: "failed" };
}

export function waitForLinearCancellation(input, signal, timeoutMs = 240000) {
  if (signal?.aborted)
    return Promise.reject(
      new Error("Linear cancellation acknowledgement was interrupted"),
    );
  return new Promise((resolve, reject) => {
    const lines = createInterface({
      input,
      terminal: false,
      crlfDelay: Infinity,
    });
    let timer;
    let finished = false;
    const finish = (error) => {
      if (finished) return;
      finished = true;
      clearTimeout(timer);
      signal?.removeEventListener("abort", aborted);
      lines.off("line", onLine);
      lines.off("close", onClose);
      lines.off("error", onError);
      lines.close();
      error ? reject(error) : resolve();
    };
    const aborted = () =>
      finish(new Error("Linear cancellation acknowledgement was interrupted"));
    const onLine = (line) =>
      finish(
        line.trim().toLowerCase() === "cancelled"
          ? null
          : new Error("Linear cancellation requires typing only cancelled"),
      );
    const onClose = () =>
      finish(new Error("Linear cancellation acknowledgement input closed"));
    const onError = () =>
      finish(new Error("Linear cancellation acknowledgement input closed"));
    lines.once("line", onLine);
    lines.once("close", onClose);
    lines.once("error", onError);
    signal?.addEventListener("abort", aborted, { once: true });
    timer = setTimeout(
      () => finish(new Error("Linear cancellation acknowledgement timed out")),
      timeoutMs,
    );
  });
}

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
  async function guards() {
    await control("disconnect");
    await disconnected();
    assert.deepEqual(await control("grant_metadata"), { connected: false });
    assert.deepEqual(await control("consent_ready"), { ready: false });
    await assert.rejects(
      control("open_authorization"),
      /Linear SDK consent is not ready/,
    );
    // Exercise real SDK readiness, then cancel without opening its browser URL.
    let settled;
    const pending = control("connect").then(
      () => (settled = { kind: "connected" }),
      (error) => (settled = consentOutcome(error)),
    );
    try {
      await ctx.waitFor(
        "Linear SDK guarded readiness",
        async () => (await control("consent_ready")).ready,
      );
      await control("disable");
      const outcome = await ctx.waitFor(
        "Linear SDK guarded cancellation",
        async () => settled,
        { timeoutMs: 5000 },
      );
      assert.equal(outcome.kind, "cancelled");
      assert.deepEqual(await control("consent_ready"), { ready: false });
      assert.deepEqual(await control("grant_metadata"), { connected: false });
      await ctx.restartHost();
      await disconnected();
      evidence.push({
        stage: "browser-control-guards",
        status: "Pass",
        actualSdkReadiness: true,
        noBrowserWithoutConsent: true,
        noGrant: true,
        restartPreservedDisconnected: true,
      });
    } finally {
      await control("disconnect");
      await pending;
    }
  }
  async function browserConsent() {
    await control("disconnect");
    await disconnected();
    const stage = (name) => {
      const entry = { stage: name, status: "Running" };
      evidence.push(entry);
      return entry;
    };
    async function begin() {
      let outcome;
      const pending = control("connect").then(
        () => (outcome = { kind: "connected" }),
        (error) => (outcome = consentOutcome(error)),
      );
      const flow = { pending, outcome: () => outcome };
      try {
        const ready = await ctx.waitFor(
          "Linear SDK consent readiness",
          async () => {
            if ((await control("consent_ready")).ready) return { ready: true };
            if (outcome) return { failed: true };
          },
          { timeoutMs: 60000 },
        );
        if (!ready.ready)
          throw new Error("Linear SDK consent creation failed before handoff");
        return flow;
      } catch (error) {
        await control("disconnect");
        await pending;
        throw error;
      }
    }
    let flow;
    try {
      const cancel = stage("human-browser-cancellation");
      flow = await begin();
      console.log(
        "LINEAR STEP 1/2: Cancel or close the first Linear consent tab WITHOUT approving. Then type cancelled and Enter here. No credentials go in this terminal.",
      );
      const opened = await control("open_authorization");
      assert.equal(opened.opened, true);
      await ctx.humanCancelled();
      await control("disconnect");
      await flow.pending;
      if (flow.outcome().kind === "connected")
        throw new Error(
          "Linear browser cancellation unexpectedly authorized the account",
        );
      if (!["cancelled", "denied"].includes(flow.outcome().kind))
        throw new Error("Linear browser sign-in did not complete");
      await reusable(opened.callbackPort);
      await disconnected();
      Object.assign(cancel, {
        status: "Pass",
        humanReportedBrowserCancellation: true,
        productionOutcome: flow.outcome().kind,
        callbackReleased: true,
        vaultEntries: 0,
      });

      const consent = stage("human-browser-consent");
      flow = await begin();
      console.log(
        "LINEAR STEP 2/2: Sign in to Linear in the new browser tab and approve READ-ONLY access. Return here; the remaining checks run automatically. No tool actions will execute.",
      );
      const accepted = await control("open_authorization");
      assert.equal(accepted.opened, true);
      const outcome = await ctx.waitFor(
        "Human Linear consent completion",
        async () => flow.outcome(),
        { timeoutMs: 305000, intervalMs: 250 },
      );
      if (outcome.kind !== "connected")
        throw new Error("Linear browser sign-in did not complete");
      await flow.pending;
      await reusable(accepted.callbackPort);
      const metadata = verifyLinearGrantMetadata(
        await control("grant_metadata"),
      );
      assert.equal(await ctx.vaultCount(), 1);
      Object.assign(consent, {
        status: "Pass",
        grant: metadata,
        callbackReleased: true,
        scopedVaultEntries: 1,
      });
      const discovery = stage("read-only-provider-discovery");
      let result;
      try {
        result = await control("discover");
      } catch {
        throw new Error("Linear read-only discovery failed");
      }
      // Never retain tool names/digest or provider descriptions. No tool is called.
      Object.assign(discovery, {
        status: "Pass",
        ...verifyLinearDiscoveryCounts(result),
      });
      const beforeRestart = verifyLinearGrantMetadata(
        await control("grant_metadata"),
      );
      await ctx.restartHost();
      const restart = stage("scoped-grant-after-restart");
      const restored = verifyLinearGrantMetadata(
        await control("grant_metadata"),
      );
      assert.deepEqual(restored, beforeRestart);
      assert.equal(await ctx.vaultCount(), 1);
      Object.assign(restart, {
        status: "Pass",
        grant: restored,
        scopedVaultEntries: 1,
      });
    } finally {
      await control("disconnect");
      if (flow) await flow.pending;
      await disconnected();
    }
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
      "mcp.linear-consent-guards": guards,
      "mcp.linear-browser-consent": browserConsent,
    },
    takeEvidence() {
      const result = evidence;
      evidence = [];
      return result;
    },
  };
}
