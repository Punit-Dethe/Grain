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
    evidence.push({
      stage,
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
  return {
    takeEvidence() {
      const value = evidence;
      evidence = [];
      return value;
    },
    handlers: {
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
