import assert from "node:assert/strict";
import { writeFile } from "node:fs/promises";
import { join } from "node:path";
import { assertWithin } from "./support.mjs";
import { authPackage, PRIVATE_MARKER } from "./auth-fixture.mjs";

export function authenticationHandlers(ctx) {
  const auth = (operation) => ctx.invoke("agent_harness_auth", { operation });
  let evidence = [];
  return {
    takeEvidence() {
      const value = evidence;
      evidence = [];
      return value;
    },
    handlers: {
      async "native.auth-fixture"() {
        const provider = ctx.provider();
        const candidate = authPackage(provider.port);
        const changed = structuredClone(candidate);
        changed.manifest.contributes.authentication.clientId =
          "another-public-client";
        await writeFile(
          assertWithin(ctx.root, join(ctx.root, "auth-fixture.grainpack")),
          JSON.stringify(changed),
        );
        await assert.rejects(
          auth("import"),
          /Unexpected account fixture declaration/,
        );
        await writeFile(
          assertWithin(ctx.root, join(ctx.root, "auth-fixture.grainpack")),
          JSON.stringify(candidate),
        );
        await auth("import");
        let removed = false;
        try {
          const main = ctx.main();
          await main.evaluate(() => {
            window.location.hash = "/extensions/installed";
          });
          await ctx.activate(
            main.getByRole("switch", {
              name: "Enable Harness Native Account",
              exact: true,
            }),
          );
          const sheet = main.getByRole("dialog");
          await sheet.waitFor({ state: "visible", timeout: 10000 });
          assert.ok((await sheet.innerText()).includes("Harness account read"));
          assert.equal((await auth("status")).enabled, false);
          assert.equal((await auth("status")).worker.count, 0);
          await ctx.activate(
            sheet.getByRole("button", {
              name: "Allow and enable",
              exact: true,
            }),
          );
          await ctx.waitFor(
            "Actual account fixture consent",
            async () => (await auth("status")).enabled,
          );

          async function connect(account) {
            provider.configure({ account });
            // Handle rejection immediately; no detached rejected IPC promise.
            const pending = auth("connect").then(
              (value) => ({ value }),
              (error) => ({ error }),
            );
            try {
              const url = await ctx.waitFor(
                "Owned authorization handoff",
                async () => await auth("authorization"),
              );
              await provider.authorize(url);
              const result = await pending;
              if (result.error) throw result.error;
              assert.equal(result.value.state, "connected");
              evidence.push({
                stage: "connected",
                account,
                pkceVerified: true,
              });
            } catch (error) {
              await auth("disconnect").catch(() => {});
              await pending;
              throw error;
            }
          }
          async function read(account) {
            const before = await ctx.status();
            const reads = provider.journal.filter(
              (entry) => entry.phase === "read",
            ).length;
            const modelStart = ctx.model().journal.length;
            const page = await ctx.request(
              account === "A" ? "account_read_a" : "account_read_b",
            );
            assert.equal(
              provider.journal.filter((entry) => entry.phase === "read").length,
              reads,
              "Read escaped approval",
            );
            await ctx.activate(
              page.locator(".agc-confirm-actions .agc-action-btn"),
            );
            await ctx.waitFor(
              "Verified authenticated read completes",
              async () => !(await ctx.status()).agent.active,
            );
            assert.equal(
              ctx.events(await ctx.status(), "dispatched", "account_read")
                .length,
              ctx.events(before, "dispatched", "account_read").length + 1,
            );
            assert.equal(
              provider.journal.filter((entry) => entry.phase === "read").length,
              reads + 1,
            );
            assert.equal(
              provider.journal.filter((entry) => entry.phase === "read").at(-1)
                .account,
              account,
            );
            assert.equal(
              ctx
                .model()
                .journal.slice(modelStart)
                .filter((entry) => entry.accountVerified).length,
              1,
              "Real tool returned the wrong account",
            );
            assert.ok(
              !(await page.locator("body").innerText()).includes(
                PRIVATE_MARKER,
              ),
            );
            evidence.push({ stage: "account-read", account, dispatchDelta: 1 });
          }
          await connect("A");
          if (ctx.fault === "abandoned-auth")
            throw new Error("Deliberately abandoned owned native credential");
          await read("A");
          const before = await ctx.status();
          const oldApproval = await ctx.request("account_read_a");
          await connect("B");
          await ctx.activate(
            oldApproval.locator(".agc-confirm-actions .agc-action-btn"),
          );
          await ctx.waitFor(
            "Old account approval refuses",
            async () => !(await ctx.status()).agent.active,
          );
          assert.equal(
            ctx.events(await ctx.status(), "dispatched", "account_read").length,
            ctx.events(before, "dispatched", "account_read").length,
          );
          await read("B");
          const exchanges = provider.journal.filter(
            (entry) => entry.phase === "exchange",
          ).length;
          await ctx.restartHost();
          assert.equal((await auth("status")).connection.state, "connected");
          assert.equal((await auth("status")).worker.count, 0);
          await read("B");
          assert.equal(
            provider.journal.filter((entry) => entry.phase === "exchange")
              .length,
            exchanges,
            "Restart silently reauthenticated",
          );
          await auth("disconnect");
          assert.equal((await auth("status")).connection.state, "disconnected");
          await ctx.waitFor(
            "Disconnect retires old account worker",
            async () => (await auth("status")).worker.count === 0,
          );
          await auth("remove");
          removed = true;
          evidence.push({
            stage: "restart-disconnect",
            retainedAccount: "B",
            removed: true,
          });
        } finally {
          if (!removed && ctx.fault !== "abandoned-auth") await auth("remove");
        }
      },
    },
  };
}
