// Production native-account schedules with external provider barriers.
// No clock/vault manipulation: expiry comes from actual OAuth responses.
import assert from "node:assert/strict";
import { createServer } from "node:net";
import { AUTH_FIXTURE_ID } from "./auth-fixture.mjs";
import { developerReload } from "./developer.mjs";

export function accountScheduleHandlers(ctx, controls) {
  const { auth, imported, review, connect, read, stale, restart, project } =
    controls;
  let evidence = [];
  const provider = () => ctx.provider();
  const count = (phase, grant) =>
    provider().journal.filter(
      (e) => e.phase === phase && (!grant || e.grant === grant),
    ).length;

  async function begin(account, options = {}) {
    provider().configure({
      account,
      deny: false,
      partial: false,
      failToken: false,
      holdToken: false,
      holdRefresh: false,
      issueRefresh: true,
      expiresIn: 1200,
      refreshExpiresIn: 1200,
      ...options,
    });
    let result;
    const pending = auth("connect").then(
      (value) => {
        result = { value };
      },
      (error) => {
        result = { error };
      },
    );
    const url = await ctx.waitFor("Pending native consent handoff", () =>
      auth("authorization"),
    );
    return {
      url,
      pending,
      async settle(failure) {
        await ctx.waitFor("Native login settles", () => result);
        await pending;
        if (failure) {
          assert.ok(result.error, "Rejected login selected a new account");
          assert.match(String(result.error), failure);
        } else {
          if (result.error) throw result.error;
          assert.equal(result.value.state, "connected");
        }
        return result;
      },
    };
  }
  async function callbackClosed(callback) {
    // An actual old callback attempt must fail, not merely leave status unchanged.
    await assert.rejects(
      fetch(callback, { redirect: "error", signal: AbortSignal.timeout(2000) }),
      (error) => error.cause?.code === "ECONNREFUSED",
    );
    const listener = createServer();
    try {
      await new Promise((resolve, reject) => {
        listener.once("error", reject);
        listener.listen(Number(callback.port), "127.0.0.1", resolve);
      });
    } finally {
      if (listener.listening)
        await new Promise((resolve, reject) =>
          listener.close((error) => (error ? reject(error) : resolve())),
        );
    }
    evidence.push({
      stage: "cancelled-callback-released",
      lateConnectionRefused: true,
      reusable: true,
    });
  }
  async function prepared() {
    await imported();
    await review();
    await connect("A");
    await read("A", "installed");
  }
  async function finish() {
    provider().release();
    await auth("cancel");
    await auth("remove");
    assert.equal(provider().heldCount, 0);
  }
  return {
    takeEvidence() {
      const value = evidence;
      evidence = [];
      return value;
    },
    handlers: {
      async "native.auth-cancellation"() {
        await prepared();
        try {
          await project("developer-a");
          await auth("load_a");
          await auth("enable");
          await connect("A");
          await read("A", "developer-a");
          for (const boundary of ["callback", "exchange"]) {
            for (const operation of ["reload", "disable", "disconnect"]) {
              const before = await ctx.vaultCount();
              const start = count("exchange");
              const flow = await begin("B", {
                holdToken: boundary === "exchange",
              });
              let callback;
              if (boundary === "callback")
                callback = await provider().consent(flow.url);
              else {
                callback = new URL(
                  new URL(flow.url).searchParams.get("redirect_uri"),
                );
                await provider().authorize(flow.url);
                await ctx.waitFor(
                  "Actual held code exchange",
                  () => provider().heldCount === 1,
                );
                assert.equal(count("exchange"), start + 1);
              }
              if (ctx.fault !== "uncancelled-login") {
                if (operation === "reload")
                  await developerReload(ctx.root, AUTH_FIXTURE_ID);
                else if (operation === "disable")
                  await ctx.invoke("extension_set_enabled", {
                    id: AUTH_FIXTURE_ID,
                    enabled: false,
                  });
                else await auth("disconnect");
              }
              await flow.settle(/cancel|changed|disabled|unavailable|removed/i);
              provider().release();
              await ctx.waitFor(
                "Held provider exchange releases",
                () => provider().heldCount === 0,
              );
              await callbackClosed(callback);
              assert.equal(
                count("exchange"),
                start + (boundary === "exchange" ? 1 : 0),
              );
              assert.equal(
                await ctx.vaultCount(),
                before - (operation === "disconnect" ? 1 : 0),
                "Cancelled candidate altered the prior vault grant",
              );
              await auth("enable");
              await connect("A");
              await read("A", "developer-a");
              await restart("A", "developer-a");
              evidence.push({
                stage: "native-cancelled",
                operation,
                boundary,
                selectedAccount: "A",
              });
            }
          }
        } finally {
          await finish();
        }
      },
      async "native.auth-switch"() {
        await prepared();
        try {
          const before = await ctx.status();
          const pending = await ctx.request("account_read_a_installed");
          await auth("disconnect");
          assert.equal((await auth("status")).connection.state, "disconnected");
          assert.equal((await auth("status")).worker.count, 0);
          await connect("B");
          await stale(pending, before, "disconnect-switch-stale-approval");
          await read("B", "installed");
          await restart("B", "installed");
          evidence.push({
            stage: "explicit-switch",
            from: "A",
            to: "B",
            oldDispatchDelta: 0,
          });
        } finally {
          await finish();
        }
      },
      async "native.auth-failed-switch"() {
        await prepared();
        try {
          for (const failure of ["cancel", "deny", "exchange"]) {
            const before = await ctx.vaultCount();
            const flow = await begin("B", {
              deny: failure === "deny",
              failToken: failure === "exchange",
            });
            if (failure === "cancel") {
              const callback = await provider().consent(flow.url);
              await auth("cancel");
              await flow.settle(/cancel/i);
              await callbackClosed(callback);
            } else {
              await provider().authorize(flow.url);
              await flow.settle(
                failure === "deny"
                  ? /denied|access_denied/i
                  : /exchange|invalid_grant/i,
              );
            }
            assert.equal(
              await ctx.vaultCount(),
              before,
              "Failed switch altered selected/candidate credentials",
            );
            assert.equal((await auth("status")).connection.state, "connected");
            await read("A", "installed");
            await restart("A", "installed");
            evidence.push({
              stage: "failed-switch-retained",
              failure,
              selectedAccount: "A",
            });
          }
          const before = await ctx.status();
          const pending = await ctx.request("account_read_a_installed");
          await connect("B");
          await stale(pending, before, "successful-switch-after-failures");
          await read("B", "installed");
          await restart("B", "installed");
        } finally {
          await finish();
        }
      },
      async "native.auth-expiry"() {
        await prepared();
        try {
          const before = await ctx.vaultCount();
          const flow = await begin("B", {
            partial: ctx.fault !== "accepted-partial-consent",
          });
          await provider().authorize(flow.url);
          await flow.settle(/declared scopes|reauthorization/i);
          assert.equal(
            await ctx.vaultCount(),
            before,
            "Partial consent selected/stored a candidate",
          );
          await read("A", "installed");
          await restart("A", "installed");
          evidence.push({
            stage: "partial-consent-refused",
            retainedAccount: "A",
          });

          const unrefreshable = await connect("B", {
            expiresIn: 2,
            issueRefresh: false,
            expectedState: "needs_reauthorization",
          });
          await ctx.waitFor(
            "Actual unrefreshable grant expiry",
            () =>
              Math.floor(Date.now() / 1000) >= Number(unrefreshable.expires_at),
          );
          const exchanges = count("exchange"),
            reads = count("read");
          assert.equal(
            (await auth("status")).connection.state,
            "needs_reauthorization",
          );
          await read("B", "installed", true);
          await ctx.restartHost();
          assert.equal(
            (await auth("status")).connection.state,
            "needs_reauthorization",
          );
          await read("B", "installed", true);
          assert.equal(count("exchange"), exchanges);
          assert.equal(count("read"), reads);
          await connect("B");
          await read("B", "installed");
          evidence.push({
            stage: "normal-expiry-no-refresh",
            apiReadDelta: 0,
            refreshDelta: 0,
            reconnectedAccount: "B",
          });

          const refreshable = await connect("A", {
            expiresIn: 2,
            expectedState: "expired",
          });
          await ctx.waitFor(
            "Actual refreshable grant expiry",
            () =>
              Math.floor(Date.now() / 1000) >= Number(refreshable.expires_at),
          );
          assert.equal((await auth("status")).connection.state, "expired");
          const refreshStart = count("exchange", "refresh_token"),
            loginStart = count("exchange", "authorization_code");
          await read("A", "installed");
          assert.equal(count("exchange", "refresh_token"), refreshStart + 1);
          assert.equal(count("exchange", "authorization_code"), loginStart);
          assert.equal((await auth("status")).connection.state, "connected");
          await restart("A", "installed");
          assert.equal(count("exchange", "refresh_token"), refreshStart + 1);
          evidence.push({
            stage: "normal-expiry-refreshed",
            refreshDelta: 1,
            loginDelta: 0,
            selectedAccount: "A",
          });
        } finally {
          await finish();
        }
      },
    },
  };
}
