// Two fixed owned issuers; Agent tools always use normal schema/approval paths.
import assert from "node:assert/strict";
import { createServer } from "node:net";
import {
  MCP_AUTH_ID,
  MCP_CLIENT_ID,
  MCP_PEER_ID,
  MCP_PEER_CLIENT_ID,
  MCP_CLIENTS,
  MCP_CLIENT_SECRETS,
} from "./mcp-oauth-fixture.mjs";

export const MCP_INDEPENDENCE_IDS = [
  "mcp.auth-provider-independence",
  "mcp.auth-fixed-port-conflict",
  "mcp.auth-issuer-binding",
];
const CONFLICT =
  "another authentication flow is already using Grain's callback port";
export function verifyProviderRead(calls, otherCalls, receipts, account, id) {
  assert.equal(calls.length, 1, "Selected provider read missing or repeated");
  assert.equal(
    calls[0].account,
    account,
    "Selected provider returned another account",
  );
  assert.equal(
    otherCalls.length,
    0,
    "Independent provider received the other provider's read",
  );
  assert.equal(
    receipts.length,
    1,
    "Actual provider result receipt missing or repeated",
  );
  assert.equal(receipts[0].mcpAccountVerified, account);
  assert.equal(
    receipts[0].mcpAccountProvider,
    id,
    "Result receipt belongs to another provider",
  );
}

async function released(raw) {
  const listener = createServer();
  await new Promise((resolve, reject) => {
    listener.once("error", reject);
    listener.listen(Number(new URL(raw).port), "127.0.0.1", resolve);
  });
  await new Promise((resolve, reject) =>
    listener.close((error) => (error ? reject(error) : resolve())),
  );
}

export function mcpIndependenceHandlers(ctx) {
  let evidence = [];
  const control = (operation, target) =>
    ctx.invoke("agent_harness_mcp", { operation, target });
  const definitions = {
    account: {
      id: MCP_AUTH_ID,
      account: "A",
      instruction: "mcp_account_a",
      peer: false,
    },
    peer: {
      id: MCP_PEER_ID,
      account: "B",
      instruction: "mcp_peer_b",
      peer: true,
    },
    client: {
      id: MCP_CLIENT_ID,
      account: "A",
      instruction: "mcp_client_a",
      peer: false,
    },
    peer_client: {
      id: MCP_PEER_CLIENT_ID,
      account: "B",
      instruction: "mcp_peer_client_b",
      peer: true,
    },
  };
  const fixture = (target) => ctx.provider(definitions[target].peer);
  const calls = (target) =>
    fixture(target).journal.filter((x) => x.method === "tools/call");
  const exchanges = (target) =>
    fixture(target).oauth.journal.filter((x) => x.phase === "token").length;
  const state = async (target) =>
    (await ctx.invoke("mcp_provider_status")).find(
      (x) => x.id === definitions[target].id,
    );
  const note = (stage, data = {}) =>
    evidence.push({ stage, status: "Pass", ...data });
  async function begin(target) {
    const f = fixture(target);
    f.configure({
      lifecycle: "stateless",
      reply: "json",
      catalog: "mixed",
      result: "normal",
      probeRejection: false,
    });
    f.oauth.configure({ account: definitions[target].account, denied: false });
    // Catch immediately: the second fixed-port flow deliberately rejects.
    const pending = control("connect", target).then(
      (value) => ({ value }),
      (error) => ({ error: String(error) }),
    );
    try {
      const url = await ctx.waitFor(
        "Owned " + target + " consent handoff",
        () => control("authorization", target),
      );
      assert.equal(await control("authorization", target), null);
      const callback = await f.oauth.authorize(url);
      return { target, callback, pending };
    } catch (error) {
      await control("disconnect", target).catch(() => {});
      await pending;
      throw error;
    }
  }
  async function finish(flow) {
    const before = exchanges(flow.target);
    assert.equal(
      (await fixture(flow.target).oauth.callback(flow.callback)).status,
      200,
    );
    const result = await flow.pending;
    if (result.error) throw new Error(result.error);
    assert.equal(result.value.connected, true);
    await released(flow.callback);
    assert.equal(exchanges(flow.target), before + 1);
    const s = await state(flow.target);
    assert.equal(s.connected, true);
    assert.equal(s.enabled, true);
    assert.equal(s.state, "stored");
    assert.equal(
      s.endpoint,
      `https://127.0.0.1:${fixture(flow.target).port}/account-mcp`,
    );
    assert.equal(await control("authorization", flow.target), null);
    note("completed-" + flow.target, { tokenExchanges: 1 });
  }
  async function read(target, stage) {
    const def = definitions[target],
      other = def.peer ? "account" : "peer";
    const before = calls(target).length,
      peerBefore = calls(other).length,
      modelStart = ctx.model().journal.length;
    const page = await ctx.request(def.instruction);
    assert.equal(calls(target).length, before, "Read escaped actual approval");
    assert.equal(calls(other).length, peerBefore);
    await ctx.activate(page.locator(".agc-confirm-actions .agc-action-btn"));
    await ctx.waitFor(
      "Independent provider read completes",
      async () => !(await ctx.status()).agent.active,
    );
    verifyProviderRead(
      calls(target).slice(before),
      calls(other).slice(peerBefore),
      ctx
        .model()
        .journal.slice(modelStart)
        .filter((x) => x.mcpAccountVerified),
      def.account,
      def.id,
    );
    await page
      .getByText("Harness MCP account " + def.account + ":", { exact: false })
      .first()
      .waitFor({ state: "visible", timeout: 10000 });
    note(stage, {
      provider: def.id,
      account: def.account,
      approvedCalls: 1,
      otherProviderCalls: 0,
    });
  }
  async function clean(targets) {
    await ctx.closePanel();
    // Disconnect invalidates pending login and scoped SDK owners, then awaits them.
    for (const target of targets) await control("disconnect", target);
    assert.equal(await ctx.vaultCount(), 0);
    assert.equal(await ctx.vaultCount(true), 0);
    for (const peer of [false, true]) {
      const f = ctx.provider(peer);
      assert.equal(f.activeSessions, 0);
      assert.equal(f.heldCalls, 0);
      assert.equal(f.delayedReplies, 0);
    }
  }
  return {
    takeEvidence() {
      const value = evidence;
      evidence = [];
      return value;
    },
    handlers: {
      async "mcp.auth-issuer-binding"() {
        const primary = ctx.provider(false),
          peer = ctx.provider(true);
        primary.oauth.configure({
          account: "A",
          denied: false,
          secretVersion: 0,
        });
        peer.oauth.configure({ account: "A", denied: false, secretVersion: 0 });
        const origin = `https://127.0.0.1:${primary.port}`,
          other = `https://127.0.0.1:${peer.port}`;
        const attempts = () =>
          peer.oauth.journal.filter((x) => x.phase === "token-attempt");
        const save = (id, secret = "") =>
          ctx.invoke("mcp_set_client_credentials", {
            id: MCP_CLIENT_ID,
            clientId: id,
            clientSecret: secret,
          });
        async function loginAt(issuer, expectRefusal = false) {
          let settled;
          const pending = control("connect", "client").then(
            (value) => (settled = { value }),
            (error) => (settled = { error: String(error) }),
          );
          try {
            const outcome = await ctx.waitFor(
              "Issuer-bound login completion or owned consent",
              async () => {
                if (settled) return settled;
                const url = await control("authorization", "client");
                return url ? { url } : settled;
              },
            );
            if (expectRefusal)
              assert.equal(
                outcome.url,
                undefined,
                "Changed issuer opened consent before registration refusal",
              );
            if (outcome.url) {
              const callback = await issuer.oauth.authorize(outcome.url);
              assert.equal((await issuer.oauth.callback(callback)).status, 200);
            }
            const result = await pending;
            if (expectRefusal)
              assert.match(
                result.error ?? "",
                /registration.*server|issuer.*changed/i,
                "Changed issuer did not refuse saved registration: " +
                  (result.error ?? "unexpected success"),
              );
            else {
              if (result.error) throw new Error(result.error);
              assert.equal(result.value.connected, true);
            }
          } finally {
            if (!settled) await control("disable", "client");
            await pending;
          }
        }
        try {
          await save(MCP_CLIENTS.confidential, MCP_CLIENT_SECRETS[0]);
          // Earlier combined cases legitimately retain peer registration metadata
          // across logout. Check preservation relative to this scoped inventory;
          // actual login/refusal independently proves the selected client's binding.
          const registrations = await ctx.vaultCount(false, true);
          assert.ok(registrations >= 1 && registrations <= 4);
          await loginAt(primary);
          peer.oauth.configure({ resourceOrigin: origin });
          primary.oauth.configure({
            authorizationServer:
              ctx.fault === "missing-mcp-issuer-rotation" ? origin : other,
          });
          // Before the fix this completes an actual code exchange with A's secret at B.
          const before = attempts().length;
          await assert.rejects(
            control("discover", "client"),
            /OAuth client registration does not match this authorization server\./,
          );
          assert.equal(
            attempts().length,
            before,
            "Stored-grant initialization sent a foreign credential",
          );
          note("stored-grant-discovery-refused", { foreignTokenRequests: 0 });
          await loginAt(peer, true);
          assert.equal(
            attempts().length,
            before,
            "Saved issuer-A credential reached issuer B",
          );
          note("issuer-change-refused-with-grant", { foreignTokenRequests: 0 });
          await control("disconnect", "client");
          await ctx.restartHost();
          assert.equal(await ctx.vaultCount(false, true), registrations);
          await loginAt(peer, true);
          assert.equal(
            attempts().length,
            before,
            "Clearing grant lost registration issuer ownership",
          );
          note("issuer-binding-survives-disconnect-restart", {
            foreignTokenRequests: 0,
          });
          peer.oauth.configure({ secretVersion: 1 });
          await save(MCP_CLIENTS.confidential, MCP_CLIENT_SECRETS[1]);
          await loginAt(peer);
          assert.equal(attempts().length, before + 1);
          assert.equal(attempts().at(-1).secretVersion, 1);
          note("explicit-new-issuer-registration", {
            tokenRequests: 1,
            secretVersion: 1,
          });
          peer.oauth.configure({ metadataUnavailable: true });
          await assert.rejects(save(MCP_CLIENTS.publicTwo), /metadata|verify/i);
          assert.equal(
            await ctx.vaultCount(),
            1,
            "Failed metadata save deleted selected grant",
          );
          assert.equal(
            await ctx.vaultCount(true),
            1,
            "Failed metadata save replaced client secret",
          );
          assert.equal(await ctx.vaultCount(false, true), registrations);
          peer.oauth.configure({ metadataUnavailable: false });
          await loginAt(peer);
          assert.equal(attempts().at(-1).secretVersion, 1);
          note("metadata-save-failure-preserves-registration-grant", {
            retainedClient: "confidential",
          });
          primary.oauth.configure({ authorizationServer: null });
          await save(MCP_CLIENTS.publicOne);
          assert.equal(await ctx.vaultCount(true), 0);
          assert.equal(await ctx.vaultCount(false, true), registrations);
          await loginAt(primary);
          await read("client", "public-client-recovery-read");
          await control("disconnect", "client");
          await ctx.restartHost();
          await loginAt(primary);
          await read("client", "same-issuer-reconnect-after-restart");
        } finally {
          primary.oauth.configure({ authorizationServer: null });
          peer.oauth.configure({
            resourceOrigin: null,
            metadataUnavailable: false,
            secretVersion: 0,
          });
          await control("disconnect", "client");
        }
      },
      async "mcp.auth-provider-independence"() {
        assert.equal(await ctx.vaultCount(), 0);
        let first, second, failure;
        const initialExchanges = [exchanges("account"), exchanges("peer")];
        try {
          assert.notEqual(ctx.provider(false).port, ctx.provider(true).port);
          // Both production login futures stay open together. Each consumes only
          // its own finite handoff; no fabricated SDK grant or shortened clock.
          const starts = await Promise.allSettled([
            begin("account"),
            begin("peer"),
          ]);
          first =
            starts[0].status === "fulfilled" ? starts[0].value : undefined;
          second =
            starts[1].status === "fulfilled" ? starts[1].value : undefined;
          const failed = starts.find((x) => x.status === "rejected");
          if (failed) throw failed.reason;
          assert.notEqual(
            new URL(first.callback).port,
            new URL(second.callback).port,
          );
          assert.deepEqual(
            [exchanges("account"), exchanges("peer")],
            initialExchanges,
          );
          assert.equal(await ctx.vaultCount(), 0);
          note("two-open-dynamic-logins", {
            distinctListeners: true,
            tokenExchanges: 0,
          });
          await finish(first);
          assert.equal((await state("peer")).connected, false);
          assert.equal((await state("peer")).enabled, false);
          assert.equal(await ctx.vaultCount(), 1);
          await read("account", "completed-account-while-peer-login-open");
          await control("disconnect", "peer");
          assert.match(
            (await second.pending).error,
            /cancel|changed|superseded/i,
          );
          await released(second.callback);
          await assert.rejects(
            fixture("peer").oauth.callback(second.callback),
            (error) => error.code === "ECONNREFUSED",
          );
          assert.equal(exchanges("peer"), initialExchanges[1]);
          assert.equal((await state("peer")).connected, false);
          assert.equal((await state("peer")).enabled, false);
          assert.equal(await ctx.vaultCount(), 1);
          await read("account", "peer-cancel-preserves-primary");
          note("peer-login-cancelled", {
            lateCallbackRefused: true,
            peerTokenExchanges: 0,
            retainedGrants: 1,
          });
          await finish(await begin("peer"));
          assert.equal(await ctx.vaultCount(), 2);
          await read("peer", "fresh-independent-B-read");
          await read("account", "original-independent-A-read");
          const counts = [exchanges("account"), exchanges("peer")];
          await ctx.restartHost();
          assert.equal(await ctx.vaultCount(), 2);
          await read("account", "primary-restart");
          await read("peer", "peer-restart");
          assert.deepEqual([exchanges("account"), exchanges("peer")], counts);
          const before = [calls("account").length, calls("peer").length],
            start = ctx.model().journal.length;
          const stale = await ctx.request("mcp_disabled_owner");
          await control("disable", "account");
          assert.equal((await state("account")).enabled, false);
          assert.equal((await state("account")).connected, true);
          assert.equal((await state("peer")).enabled, true);
          await ctx.activate(
            stale.locator(".agc-confirm-actions .agc-action-btn"),
          );
          await ctx.waitFor(
            "Disabled owner's stale approval refused",
            async () => !(await ctx.status()).agent.active,
          );
          assert.deepEqual(
            [calls("account").length, calls("peer").length],
            before,
          );
          assert.equal(
            ctx
              .model()
              .journal.slice(start)
              .filter((x) => x.mcpAccountRefused).length,
            1,
          );
          await assert.rejects(control("discover", "account"), /disabled/);
          await read("peer", "primary-disable-preserves-peer");
          await control("disconnect", "account");
          assert.equal(await ctx.vaultCount(), 1);
          await ctx.restartHost();
          assert.equal((await state("account")).connected, false);
          await read("peer", "primary-disconnect-peer-restart");
          assert.deepEqual([exchanges("account"), exchanges("peer")], counts);
          note("independent-disable-disconnect", {
            staleWireCalls: 0,
            retainedGrants: 1,
            newLoginAfterRestart: 0,
          });
        } catch (error) {
          failure = error;
          throw error;
        } finally {
          try {
            await clean(["account", "peer"]);
          } catch (error) {
            if (!failure) throw error;
            evidence.push({
              stage: "independence-cleanup",
              status: "Fail",
              error: error.message,
            });
          }
          await Promise.all([first?.pending, second?.pending]);
        }
      },
      async "mcp.auth-fixed-port-conflict"() {
        assert.equal(await ctx.vaultCount(), 0);
        let first, failure;
        const initialRegistrations = [
          fixture("client"),
          fixture("peer_client"),
        ].map(
          (f) => f.oauth.journal.filter((x) => x.phase === "registered").length,
        );
        const peerExchanges = exchanges("peer_client");
        try {
          for (const target of ["client", "peer_client"])
            await ctx.invoke("mcp_set_client_credentials", {
              id: definitions[target].id,
              clientId: MCP_CLIENTS.publicOne,
              clientSecret: "",
            });
          first = await begin("client");
          assert.equal(new URL(first.callback).port, "31938");
          const peerJournal = fixture("peer_client").oauth.journal.length;
          let rejection;
          if (ctx.fault !== "skip-fixed-port-conflict") {
            try {
              await control("connect", "peer_client");
            } catch (error) {
              rejection = String(error);
            }
          }
          assert.ok(
            rejection?.includes(CONFLICT),
            "Second fixed-port login was not explicitly refused",
          );
          assert.equal(await control("authorization", "peer_client"), null);
          assert.equal(
            fixture("peer_client").oauth.journal.length,
            peerJournal,
          );
          assert.equal(exchanges("peer_client"), peerExchanges);
          assert.equal((await state("peer_client")).connected, false);
          assert.equal((await state("peer_client")).enabled, false);
          assert.equal(await ctx.vaultCount(), 0);
          note("fixed-port-conflict", {
            refused: true,
            peerTokenExchanges: 0,
            peerEnabled: false,
            primaryFlowPreserved: true,
          });
          await finish(first);
          await read("client", "first-fixed-provider-after-conflict");
          const serial = await begin("peer_client");
          assert.equal(new URL(serial.callback).port, "31938");
          await finish(serial);
          assert.equal(await ctx.vaultCount(), 2);
          await read("peer_client", "serial-fixed-peer-B");
          await read("client", "fixed-primary-A-still-usable");
          const counts = [exchanges("client"), exchanges("peer_client")];
          await ctx.restartHost();
          assert.equal(await ctx.vaultCount(), 2);
          await read("client", "fixed-primary-restart");
          await read("peer_client", "fixed-peer-restart");
          assert.deepEqual(
            [exchanges("client"), exchanges("peer_client")],
            counts,
          );
          assert.deepEqual(
            [fixture("client"), fixture("peer_client")].map(
              (f) =>
                f.oauth.journal.filter((x) => x.phase === "registered").length,
            ),
            initialRegistrations,
          );
          note("serial-fixed-port-recovery", {
            storedGrants: 2,
            dynamicRegistrations: 0,
            newLoginAfterRestart: 0,
          });
        } catch (error) {
          failure = error;
          throw error;
        } finally {
          try {
            await clean(["client", "peer_client"]);
          } catch (error) {
            if (!failure) throw error;
            evidence.push({
              stage: "fixed-port-cleanup",
              status: "Fail",
              error: error.message,
            });
          }
          await first?.pending;
        }
      },
    },
  };
}
