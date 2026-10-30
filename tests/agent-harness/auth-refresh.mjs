// Real logout/refresh and per-provider concurrency; no vault or clock edits.
import assert from "node:assert/strict";
import { writeFile } from "node:fs/promises";
import { join } from "node:path";
import {
  peerAuthPackage,
  PEER_FIXTURE_ID,
  PRIVATE_MARKER,
} from "./auth-fixture.mjs";

export function accountRefreshHandlers(ctx, controls) {
  const { auth, imported, review, connect, read, restart } = controls;
  const peer = (operation) =>
    ctx.invoke("agent_harness_auth", { operation, target: "peer" });
  const provider = () => ctx.provider();
  const count = (phase, target = "primary", grant) =>
    provider().journal.filter(
      (e) =>
        e.phase === phase &&
        e.provider === target &&
        (!grant || e.grant === grant),
    ).length;
  let evidence = [];

  async function expiredPrimary() {
    const grant = await connect("A", {
      expiresIn: 2,
      holdRefresh: true,
      expectedState: "expired",
    });
    await ctx.waitFor(
      "Actual held-refresh grant expires",
      () => Math.floor(Date.now() / 1000) >= Number(grant.expires_at),
    );
  }
  function probe() {
    // The returned result cannot contain a token. Catch immediately, including
    // on assertion failure, so final cleanup can release and await the request.
    return auth("refresh").then(
      (value) => ({ value }),
      (error) => ({ error }),
    );
  }
  async function held() {
    await ctx.waitFor(
      "Production refresh reaches external barrier",
      () => provider().heldCount === 1,
    );
  }
  async function installPeer() {
    await writeFile(
      join(ctx.root, "auth-peer.grainpack"),
      JSON.stringify(peerAuthPackage(provider().port)),
    );
    await peer("import");
    await ctx.activate(ctx.main().locator('.nav-item[data-page="agent"]'));
    await ctx.main().evaluate(() => {
      window.location.hash = "/extensions/installed";
    });
    await ctx.activate(
      ctx.main().getByRole("switch", {
        name: "Enable Harness Native Peer",
        exact: true,
      }),
    );
    const sheet = ctx.main().getByRole("dialog");
    await sheet.waitFor({ state: "visible", timeout: 10000 });
    const text = await sheet.innerText();
    assert.ok(
      text.includes("Connect Harness Peer OAuth") &&
        text.includes("fixture.peer.read") &&
        text.includes("127.0.0.1"),
    );
    assert.equal((await peer("status")).enabled, false);
    await ctx.activate(
      sheet.getByRole("button", { name: "Allow and enable", exact: true }),
    );
    await ctx.waitFor(
      "Peer declaration explicitly reviewed",
      async () => (await peer("status")).enabled,
    );
    const pending = peer("connect").then(
      (value) => ({ value }),
      (error) => ({ error }),
    );
    try {
      const url = await ctx.waitFor("Independent peer consent handoff", () =>
        peer("authorization"),
      );
      await provider().authorize(url);
      const result = await pending;
      if (result.error) throw result.error;
      assert.equal(result.value.state, "connected");
      assert.deepEqual(result.value.granted_scopes, ["fixture.peer.read"]);
    } catch (error) {
      await peer("cancel");
      await pending;
      throw error;
    }
  }
  async function readPeer() {
    const modelStart = ctx.model().journal.length,
      reads = count("read", "peer"),
      before = await ctx.status();
    const page = await ctx.request("account_read_b_peer");
    assert.equal(count("read", "peer"), reads, "Peer read escaped approval");
    await ctx.activate(page.locator(".agc-confirm-actions .agc-action-btn"));
    await ctx.waitFor(
      "Actual peer account read completes",
      async () => !(await ctx.status()).agent.active,
    );
    assert.equal(count("read", "peer"), reads + 1);
    assert.equal(
      ctx
        .model()
        .journal.slice(modelStart)
        .filter((e) => e.accountVerified).length,
      1,
    );
    assert.equal(
      ctx.events(await ctx.status(), "dispatched", "account_read").length,
      ctx.events(before, "dispatched", "account_read").length + 1,
    );
    assert.equal(
      ctx
        .events(await ctx.status(), "dispatched", "account_read")
        .filter((e) => e.extension === PEER_FIXTURE_ID).length,
      ctx
        .events(before, "dispatched", "account_read")
        .filter((e) => e.extension === PEER_FIXTURE_ID).length + 1,
      "Peer dispatch receipt belongs to the wrong provider",
    );
    assert.ok(
      !(await page.locator("body").innerText()).includes(PRIVATE_MARKER),
    );
  }
  return {
    takeEvidence() {
      const value = evidence;
      evidence = [];
      return value;
    },
    handlers: {
      async "native.auth-refresh-logout"() {
        let pending;
        await imported();
        await review();
        const baseline = await ctx.vaultCount();
        try {
          // First use a real Agent-approved read, interrupted by actual logout.
          await expiredPrimary();
          const before = await ctx.status(),
            modelStart = ctx.model().journal.length,
            reads = count("read"),
            refreshes = count("exchange", "primary", "refresh_token");
          const page = await ctx.request("account_read_a_installed");
          assert.equal(
            count("exchange", "primary", "refresh_token"),
            refreshes,
          );
          await ctx.activate(
            page.locator(".agc-confirm-actions .agc-action-btn"),
          );
          await held();
          await auth("disconnect");
          assert.equal((await auth("status")).connection.state, "disconnected");
          assert.equal((await auth("status")).worker.count, 0);
          await connect("B");
          provider().release();
          await ctx.waitFor(
            "Old approved call retires",
            async () => !(await ctx.status()).agent.active,
          );
          assert.equal(
            count("read"),
            reads,
            "Old refresh authorized a post-logout read",
          );
          assert.equal(
            count("exchange", "primary", "refresh_token"),
            refreshes + 1,
            "Interrupted refresh was replayed",
          );
          assert.equal(
            ctx
              .model()
              .journal.slice(modelStart)
              .filter((e) => e.accountVerified).length,
            0,
          );
          assert.equal(
            ctx
              .model()
              .journal.slice(modelStart)
              .filter((e) => e.accountUnknown).length,
            1,
            "Interrupted approved read did not yield the actual unknown outcome",
          );
          assert.equal(
            ctx.events(await ctx.status(), "dispatched", "account_read").length,
            ctx.events(before, "dispatched", "account_read").length + 1,
          );
          assert.match(
            await page.locator("body").innerText(),
            /unknown|unconfirmed|could not confirm|no longer|failed|changed/i,
          );
          await read("B", "installed");
          await restart("B", "installed");
          evidence.push({
            stage: "approved-refresh-logout-reconnect",
            oldApiReadDelta: 0,
            oldDispatchDelta: 1,
            refreshDelta: 1,
            selectedAccount: "B",
          });

          // Await the actual auth future after logout, proving completion cannot
          // republish a discarded grant even when no replacement exists yet.
          await expiredPrimary();
          pending = probe();
          await held();
          await auth("disconnect");
          assert.equal(await ctx.vaultCount(), baseline);
          provider().release();
          const discarded = await pending;
          pending = null;
          assert.ok(
            discarded.error,
            "Late refresh restored the logged-out grant",
          );
          assert.match(
            String(discarded.error),
            /changed|removed|not connected|unavailable/i,
          );
          assert.equal((await auth("status")).connection.state, "disconnected");
          assert.equal(await ctx.vaultCount(), baseline);
          await connect("B");
          await read("B", "installed");
          evidence.push({
            stage: "discarded-refresh-settled",
            publicationRefused: true,
            loggedOutVaultDelta: 0,
            selectedAccount: "B",
          });

          await installPeer();
          await expiredPrimary();
          const primaryReads = count("read"),
            peerRefreshes = count("exchange", "peer", "refresh_token");
          pending = probe();
          await held();
          const hostSession = (await ctx.status()).hostSession;
          if (ctx.fault === "released-refresh") provider().release();
          await readPeer();
          assert.equal(
            provider().heldCount,
            1,
            "Peer read was not completed while the primary refresh remained held",
          );
          assert.equal(
            (await ctx.status()).hostSession,
            hostSession,
            "Providers ran in different hosts",
          );
          assert.equal(count("read"), primaryReads);
          assert.equal(
            count("exchange", "peer", "refresh_token"),
            peerRefreshes,
          );
          provider().release();
          const refreshed = await pending;
          pending = null;
          if (refreshed.error) throw refreshed.error;
          assert.equal(refreshed.value, null, "Refresh probe exported data");
          assert.equal((await auth("status")).connection.state, "connected");
          assert.equal(await ctx.vaultCount(), baseline + 2);
          await read("A", "installed");
          const exchanges = count("exchange") + count("exchange", "peer");
          await ctx.restartHost();
          await read("A", "installed");
          await readPeer();
          assert.equal(
            count("exchange") + count("exchange", "peer"),
            exchanges,
            "Restart reauthenticated either provider",
          );
          await auth("remove");
          assert.equal(await ctx.vaultCount(), baseline + 1);
          assert.equal((await peer("status")).connection.state, "connected");
          await readPeer();
          evidence.push({
            stage: "independent-provider-refresh",
            sameHost: true,
            peerReadBeforePrimaryRelease: true,
            refreshProbeExportedData: false,
            restartReauthenticationDelta: 0,
            peerSurvivesPrimaryRemoval: true,
          });
        } finally {
          provider().release();
          if (pending) await pending;
          await ctx.closePanel();
          await auth("remove");
          await peer("remove");
          assert.equal(provider().heldCount, 0);
          assert.equal(await ctx.vaultCount(), baseline);
        }
      },
    },
  };
}
