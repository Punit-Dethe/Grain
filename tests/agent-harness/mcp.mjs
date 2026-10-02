import assert from "node:assert/strict";

export function mcpHandlers(ctx) {
  let evidence = [];
  const control = (operation) => ctx.invoke("agent_harness_mcp", { operation });
  const calls = () =>
    ctx.provider().journal.filter((entry) => entry.method === "tools/call")
      .length;
  async function enabled() {
    await control("enable");
    const status = await ctx.invoke("mcp_provider_status");
    const fixture = status.find((item) => item.id === "grain-harness");
    assert.equal(fixture.state, "fixture_no_auth");
    assert.equal(
      fixture.connected,
      false,
      "Test peer fabricated stored credentials",
    );
    assert.equal(fixture.enabled, true);
    assert.equal(
      fixture.endpoint,
      `https://127.0.0.1:${ctx.provider().port}/mcp`,
    );
    await assert.rejects(
      ctx.invoke("mcp_connect_provider", { id: "grain-harness" }),
      /no account to connect/,
    );
    await assert.rejects(
      ctx.invoke("mcp_disconnect_provider", { id: "grain-harness" }),
      /no account/,
    );
    await assert.rejects(
      ctx.invoke("mcp_set_client_credentials", {
        id: "grain-harness",
        clientId: "unused",
        clientSecret: "",
      }),
      /does not require developer OAuth/,
    );
  }
  async function read(stage) {
    const observation = { stage, status: "Running" };
    evidence.push(observation);
    const before = calls(),
      modelStart = ctx.model().journal.length;
    const page = await ctx.request("mcp_read");
    assert.equal(calls(), before, "MCP tool escaped approval");
    await ctx.activate(page.locator(".agc-confirm-actions .agc-action-btn"));
    await ctx.waitFor(
      "Actual MCP read completes",
      async () => !(await ctx.status()).agent.active,
    );
    assert.equal(
      calls(),
      before + 1,
      "Approved MCP call was replayed or never received",
    );
    assert.equal(
      ctx
        .model()
        .journal.slice(modelStart)
        .filter((entry) => entry.mcpVerified).length,
      1,
      "MCP result was not verified by the model oracle",
    );
    await page
      .getByText("Harness MCP reply:", { exact: false })
      .first()
      .waitFor({ state: "visible", timeout: 10000 });
    assert.equal(
      ctx.provider().activeSessions,
      0,
      "Operation retained an MCP session",
    );
    assert.ok(
      !ctx.provider().journal.some((entry) => entry.phase === "error"),
      "MCP fixture rejected real wire traffic",
    );
    Object.assign(observation, {
      stage,
      status: "Pass",
      wireCalls: 1,
      approvalRequired: true,
      nestedWireAndResultVerified: true,
      activeSessions: 0,
    });
  }
  async function stale(operation) {
    const before = calls(),
      modelStart = ctx.model().journal.length;
    const page = await ctx.request("mcp_read");
    await operation();
    await ctx.activate(page.locator(".agc-confirm-actions .agc-action-btn"));
    await ctx.waitFor(
      "Stale MCP approval refuses",
      async () => !(await ctx.status()).agent.active,
    );
    assert.equal(calls(), before, "Stale MCP approval reached the provider");
    assert.equal(
      ctx
        .model()
        .journal.slice(modelStart)
        .filter((entry) => entry.mcpRefused).length,
      1,
    );
    evidence.push({ stage: "stale-approval", wireCalls: 0 });
  }
  async function outcome(
    stage,
    instruction,
    flag,
    visibleText,
    timeoutMs = 20000,
  ) {
    const observation = { stage, status: "Running" };
    evidence.push(observation);
    const before = calls(),
      modelStart = ctx.model().journal.length;
    const page = await ctx.request(instruction);
    assert.equal(calls(), before, `${stage}: call escaped approval`);
    await ctx.activate(page.locator(".agc-confirm-actions .agc-action-btn"));
    await ctx.waitFor(
      `${stage}: actual Agent completes`,
      async () => !(await ctx.status()).agent.active,
      { timeoutMs },
    );
    assert.equal(
      calls(),
      before + 1,
      `${stage}: dispatched call absent or replayed`,
    );
    assert.equal(
      ctx
        .model()
        .journal.slice(modelStart)
        .filter((entry) => entry[flag]).length,
      1,
      `${stage}: wrong result classification`,
    );
    await page
      .getByText(visibleText, { exact: false })
      .first()
      .waitFor({ state: "visible", timeout: 10000 });
    assert.equal(
      ctx.provider().activeSessions,
      0,
      `${stage}: session survived`,
    );
    assert.ok(
      !ctx.provider().journal.some((entry) => entry.phase === "error"),
      `${stage}: fixture rejected wire traffic`,
    );
    Object.assign(observation, {
      status: "Pass",
      wireCalls: 1,
      approvalRequired: true,
      modelOracle: flag,
      activeSessions: 0,
    });
  }
  async function refusedCatalog(stage, expected, timeoutMs = 20000) {
    const before = calls(),
      modelStart = ctx.model().journal.length;
    await ctx.closePanel();
    await ctx.invoke("agent_harness_submit", {
      instruction: "mcp_catalog_refusal",
    });
    const page = await ctx.panel();
    await ctx.waitFor(
      `${stage}: actual Agent refuses catalog`,
      async () =>
        ctx
          .model()
          .journal.slice(modelStart)
          .some(
            (entry) => entry.mcpCatalogRefused || entry.state === "error",
          ) && !(await ctx.status()).agent.active,
      { timeoutMs },
    );
    const entries = ctx.model().journal.slice(modelStart);
    const refusal = entries.filter((entry) => entry.mcpCatalogRefused);
    assert.equal(
      refusal.length,
      1,
      `${stage}: missing independent model refusal`,
    );
    assert.match(refusal[0].mcpCatalogRefused, expected);
    assert.ok(
      entries.every(
        (entry) => !entry.offered?.some((name) => name.startsWith("act__")),
      ),
      `${stage}: failed catalog exposed actions`,
    );
    assert.equal(
      calls(),
      before,
      `${stage}: rejected catalog dispatched a tool`,
    );
    assert.equal((await ctx.status()).agent.pendingApproval, false);
    await page
      .getByText("Harness verified MCP catalog refusal", { exact: false })
      .first()
      .waitFor({ state: "visible", timeout: 10000 });
    assert.equal(
      ctx.provider().activeSessions,
      0,
      `${stage}: failed discovery retained session`,
    );
    assert.equal(
      ctx.provider().delayedReplies,
      0,
      `${stage}: delayed reply timer survived`,
    );
    assert.ok(
      !ctx.provider().journal.some((entry) => entry.phase === "error"),
      `${stage}: fixture rejected traffic`,
    );
    return refusal[0].mcpCatalogRefused;
  }
  return {
    takeEvidence() {
      const value = evidence;
      evidence = [];
      return value;
    },
    handlers: {
      async "mcp.catalog-budgets"() {
        const provider = ctx.provider();
        const variants = [
          ["tool_count", /128-tool limit/, () => 1],
          ["duplicate", /duplicate tool names/, () => 2],
          ["cursor_size", /cursor exceeds the safety limit/, () => 1],
          ["empty_pages", /catalog exceeds the page limit/, () => 32],
          [
            "catalog_bytes",
            /catalog exceeds the metadata byte limit/,
            (reply) => (reply === "json" ? 2 : 7),
          ],
          [
            "operation_bytes",
            /catalog request failed; the catalog is incomplete/,
            (reply) => (reply === "json" ? 8 : 24),
          ],
        ];
        await enabled();
        try {
          for (const lifecycle of ["stateless", "legacy"]) {
            for (const reply of ["json", "sse"]) {
              for (const [catalog, expected, pages] of variants) {
                const stage = `${lifecycle}-${reply}-${catalog}`;
                const observation = { stage, status: "Running" };
                evidence.push(observation);
                provider.configure({
                  lifecycle,
                  reply,
                  catalog,
                  revision: "one",
                  result: "normal",
                });
                const before = calls();
                const testStart = provider.journal.length;
                await assert.rejects(
                  control("discover"),
                  expected,
                  `${stage}: management Test accepted incomplete catalog`,
                );
                const testRequests = provider.journal.slice(testStart);
                assert.equal(
                  testRequests.filter((entry) => entry.method === "tools/list")
                    .length,
                  pages(reply),
                  `${stage}: unexpected management page count`,
                );
                const agentStart = provider.journal.length;
                const reason = await refusedCatalog(stage, expected);
                const agentRequests = provider.journal.slice(agentStart);
                assert.equal(
                  agentRequests.filter((entry) => entry.method === "tools/list")
                    .length,
                  pages(reply),
                  `${stage}: unexpected Agent page count`,
                );
                for (const requests of [testRequests, agentRequests]) {
                  const lists = requests.filter(
                    (entry) => entry.method === "tools/list",
                  );
                  assert.ok(
                    lists.every(
                      (entry) =>
                        entry.responseBytes <
                        (reply === "json" ? 2 * 1024 * 1024 : 512 * 1024),
                    ),
                    `${stage}: individual body limit masked catalog/operation limit`,
                  );
                  if (catalog === "operation_bytes")
                    assert.ok(
                      requests.reduce(
                        (total, entry) => total + (entry.responseBytes ?? 0),
                        0,
                      ) >
                        8 * 1024 * 1024,
                      `${stage}: wire replies never crossed cumulative limit`,
                    );
                }
                assert.equal(calls(), before);
                Object.assign(observation, {
                  status: "Pass",
                  managementPages: pages(reply),
                  agentPages: pages(reply),
                  wireCalls: 0,
                  refusal: reason,
                  activeSessions: 0,
                });
                provider.configure({ catalog: "mixed" });
                await read(`${stage}-fresh-recovery`);
              }
            }
          }
        } finally {
          await ctx.closePanel();
          await control("disable");
        }
      },
      async "mcp.http-deadline"() {
        const provider = ctx.provider();
        await enabled();
        try {
          for (const lifecycle of ["stateless", "legacy"]) {
            for (const reply of ["json", "sse"]) {
              const stage = `${lifecycle}-${reply}-45s-http-deadline`;
              provider.configure({
                lifecycle,
                reply,
                catalog: "mixed",
                revision: "one",
                result: "held",
              });
              const start = provider.journal.length;
              await outcome(
                stage,
                "mcp_unknown",
                "mcpUnknownVerified",
                "could not confirm",
                60000,
              );
              const received = provider.journal
                .slice(start)
                .filter((entry) => entry.method === "tools/call");
              assert.equal(received.length, 1);
              const elapsedMs = Math.round(
                performance.now() - received[0].receivedAtMs,
              );
              assert.ok(
                elapsedMs >= 44000 && elapsedMs < 55000,
                `${stage}: production HTTP deadline measured ${elapsedMs}ms`,
              );
              await ctx.waitFor(
                `${stage}: transport actually closes`,
                () => provider.heldCalls === 0,
              );
              provider.attemptLateReply();
              evidence.push({
                stage: `${stage}-measured`,
                status: "Pass",
                elapsedMs,
                wireCalls: 1,
                heldCalls: 0,
                activeSessions: 0,
              });
              provider.configure({ result: "normal" });
              await read(`${stage}-fresh-recovery`);
            }
          }
        } finally {
          await ctx.closePanel();
          await control("disable");
        }
      },
      async "mcp.discovery-deadline"() {
        const provider = ctx.provider();
        await enabled();
        try {
          for (const [lifecycle, reply] of [
            ["stateless", "sse"],
            ["legacy", "json"],
          ]) {
            const stage = `${lifecycle}-${reply}-90s-absolute-discovery`;
            const observation = { stage, status: "Running" };
            evidence.push(observation);
            provider.configure({
              lifecycle,
              reply,
              catalog: "slow_pages",
              revision: "one",
              result: "normal",
            });
            const start = provider.journal.length,
              began = performance.now();
            const refusal = await refusedCatalog(
              stage,
              /MCP discovery timed out; the catalog is incomplete/,
              105000,
            );
            const elapsedMs = Math.round(performance.now() - began);
            assert.ok(
              elapsedMs >= 89000 && elapsedMs < 100000,
              `${stage}: production absolute deadline measured ${elapsedMs}ms`,
            );
            const lists = provider.journal
              .slice(start)
              .filter((entry) => entry.method === "tools/list");
            assert.equal(
              lists.length,
              3,
              `${stage}: did not exercise successive sub-45s pages`,
            );
            assert.equal(
              lists.filter((entry) => entry.phase === "catalog-delivered")
                .length,
              2,
            );
            assert.ok(
              provider.journal
                .slice(start)
                .some((entry) => entry.phase === "catalog-delay-closed"),
              `${stage}: final delayed request survived timeout`,
            );
            Object.assign(observation, {
              status: "Pass",
              elapsedMs,
              completedPages: 2,
              attemptedPages: 3,
              wireCalls: 0,
              delayedReplies: 0,
              activeSessions: 0,
              refusal,
            });
            provider.configure({ catalog: "mixed" });
            await read(`${stage}-fresh-recovery`);
          }
        } finally {
          await ctx.closePanel();
          await control("disable");
        }
      },
      async "mcp.response-preview"() {
        const provider = ctx.provider();
        await enabled();
        try {
          for (const reply of ["json", "sse"]) {
            provider.configure({
              lifecycle: "stateless",
              reply,
              catalog: "mixed",
              revision: "one",
              result: "preview",
            });
            await outcome(
              `${reply}-utf8-and-structured-preview`,
              "mcp_preview",
              "mcpPreviewVerified",
              "Some result data was omitted because of the size limit.",
            );
            provider.configure({ result: "normal" });
            await read(`${reply}-preview-recovery`);
          }
        } finally {
          await ctx.closePanel();
          await control("disable");
        }
      },
      async "mcp.transport-bounds"() {
        const provider = ctx.provider();
        await enabled();
        try {
          for (const lifecycle of ["stateless", "legacy"]) {
            for (const kind of [
              "json_declared",
              "json_chunked",
              "sse_data",
              "sse_comments",
              "http_error",
              "drop",
            ]) {
              provider.configure({
                lifecycle,
                reply: "json",
                catalog: "mixed",
                revision: "one",
                result: kind === "drop" ? kind : `overflow_${kind}`,
              });
              await outcome(
                `${lifecycle}-${kind}`,
                "mcp_unknown",
                "mcpUnknownVerified",
                "could not confirm",
              );
              provider.configure({ result: "normal" });
              await read(`${lifecycle}-${kind}-recovery`);
            }
          }
        } finally {
          await ctx.closePanel();
          await control("disable");
        }
      },
      async "mcp.close-cancellation"() {
        const provider = ctx.provider();
        await enabled();
        try {
          for (const lifecycle of ["stateless", "legacy"]) {
            for (const reply of ["json", "sse"]) {
              const stage = `${lifecycle}-${reply}-close-held`;
              const observation = { stage, status: "Running" };
              evidence.push(observation);
              provider.configure({
                lifecycle,
                reply,
                catalog: "mixed",
                revision: "one",
                result: "held",
              });
              const before = calls(),
                modelStart = ctx.model().journal.length;
              const page = await ctx.request("mcp_read");
              assert.equal(calls(), before, `${stage}: call escaped approval`);
              await ctx.activate(
                page.locator(".agc-confirm-actions .agc-action-btn"),
              );
              await ctx.waitFor(
                `${stage}: provider receives approved call`,
                () => provider.heldCalls === 1,
              );
              assert.equal(calls(), before + 1);
              await ctx.closePanel();
              await ctx.waitFor(
                `${stage}: HTTP reply closes and session retires`,
                () => provider.heldCalls === 0 && provider.activeSessions === 0,
              );
              provider.attemptLateReply();
              assert.equal(
                calls(),
                before + 1,
                `${stage}: cancelled call replayed`,
              );
              assert.equal(
                ctx
                  .model()
                  .journal.slice(modelStart)
                  .filter((entry) => entry.mcpVerified).length,
                0,
                `${stage}: late success reached model`,
              );
              assert.ok(
                !provider.journal.some((entry) => entry.phase === "error"),
                `${stage}: fixture wire error`,
              );
              provider.configure({ result: "normal" });
              await read(`${stage}-fresh-session`);
              assert.equal(
                ctx
                  .model()
                  .journal.slice(modelStart)
                  .filter((entry) => entry.mcpVerified).length,
                1,
                `${stage}: late result contaminated fresh session`,
              );
              Object.assign(observation, {
                status: "Pass",
                cancelledWireCalls: 1,
                freshWireCalls: 1,
                lateReplyDiscarded: true,
                activeSessions: 0,
                heldCalls: 0,
              });
            }
            const before = calls();
            await ctx.request("mcp_read");
            await ctx.closePanel();
            assert.equal(
              calls(),
              before,
              `${lifecycle}: closed pending approval dispatched`,
            );
            evidence.push({
              stage: `${lifecycle}-close-pending`,
              status: "Pass",
              wireCalls: 0,
            });
            await read(`${lifecycle}-pending-close-recovery`);
          }
        } finally {
          await ctx.closePanel();
          await control("disable");
        }
      },
      async "mcp.transport-contract"() {
        const provider = ctx.provider();
        const start = provider.journal.length;
        await enabled();
        assert.equal(
          provider.journal.length,
          start,
          "Enabling started an idle MCP connection",
        );
        try {
          for (const lifecycle of ["stateless", "legacy"]) {
            for (const reply of ["json", "sse"]) {
              provider.configure({
                lifecycle,
                reply,
                catalog: "mixed",
                revision: "one",
              });
              const before = provider.journal.length;
              const discovered = await control("discover");
              assert.deepEqual(discovered.tools, [
                "fixture_read",
                "fixture_other",
              ]);
              assert.equal(discovered.tool_count, 2);
              await read(`${lifecycle}-${reply}`);
              const requests = provider.journal.slice(before);
              assert.equal(
                requests.filter((entry) => entry.method === "tools/list")
                  .length,
                8,
                "Discovery/search/load/revalidation did not complete both pages",
              );
              if (lifecycle === "stateless") {
                assert.equal(
                  requests.filter((entry) => entry.method === "initialize")
                    .length,
                  0,
                );
                assert.equal(
                  requests.filter((entry) => entry.requestMetadataVerified)
                    .length,
                  9,
                );
              } else {
                assert.equal(
                  requests.filter((entry) => entry.method === "initialize")
                    .length,
                  4,
                );
                assert.equal(
                  requests.filter((entry) => entry.phase === "session-deleted")
                    .length,
                  4,
                );
              }
            }
          }
          await ctx.closePanel();
          await ctx.restartHost();
          await read("restart-preserves-enabled-no-auth-peer");
          await stale(() => control("disable"));
          await enabled();
          await read("fresh-after-disable");
        } finally {
          await ctx.closePanel();
          await control("disable");
        }
      },
      async "mcp.mixed-catalog"() {
        const provider = ctx.provider();
        provider.configure({
          lifecycle: "stateless",
          reply: "json",
          catalog: "mixed",
          revision: "one",
        });
        await enabled();
        try {
          const discovered = await control("discover");
          assert.deepEqual(discovered.tools, ["fixture_read", "fixture_other"]);
          for (const name of ["excluded_remote", "excluded_dialect"])
            assert.ok(
              ctx
                .log()
                .includes(
                  `MCP tool '${name}' excluded from the supported catalog`,
                ),
              "Developer warning missing for an excluded MCP schema",
            );
          await read("supported-from-mixed-paged-catalog");
          await ctx.closePanel();
          const before = calls(),
            modelStart = ctx.model().journal.length;
          await ctx.invoke("agent_harness_submit", {
            instruction: "mcp_excluded",
          });
          await ctx.waitFor(
            "Excluded MCP selection refuses",
            async () =>
              ctx
                .model()
                .journal.slice(modelStart)
                .some((entry) => entry.mcpExcludedVerified) &&
              !(await ctx.status()).agent.active,
          );
          assert.equal(calls(), before);
          assert.equal(
            ctx
              .model()
              .journal.slice(modelStart)
              .filter((entry) => entry.mcpExcludedVerified).length,
            1,
          );
          assert.equal((await ctx.status()).agent.pendingApproval, false);
          evidence.push({ stage: "excluded-schema-not-offered", wireCalls: 0 });
          provider.configure({ catalog: "unsupported" });
          assert.equal((await control("discover")).tool_count, 0);
          evidence.push({
            stage: "entirely-unsupported-catalog",
            supportedTools: 0,
          });
          provider.configure({ catalog: "repeat_cursor" });
          await assert.rejects(
            control("discover"),
            /pagination repeated a cursor/,
          );
          assert.equal(calls(), before);
          provider.configure({ catalog: "mixed" });
          await stale(async () => provider.configure({ revision: "two" }));
          await read("fresh-after-schema-change-and-pagination-refusal");
        } finally {
          await ctx.closePanel();
          await control("disable");
        }
      },
    },
  };
}
