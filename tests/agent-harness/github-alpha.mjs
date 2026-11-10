// Discover the bundled GitHub catalogue in the installed test app through Tauri IPC.
import assert from "node:assert/strict";
import { spawn, execFile } from "node:child_process";
import { promisify } from "node:util";
import { createServer } from "node:net";
import { mkdtemp, mkdir, realpath } from "node:fs/promises";
import { join, dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { randomUUID } from "node:crypto";
import { chromium } from "@playwright/test";
import { waitFor, writeReport, Blocked } from "./support.mjs";
import { hashFile } from "./stamp.mjs";

// Match only host-owned constant messages; never retain arbitrary IPC/provider text.
export function mcpFailureCode(error) {
  const message = String(error);
  for (const [text, code] of [
    [
      "MCP authentication is temporarily unavailable.",
      "authentication_unavailable",
    ],
    ["The MCP account needs sign-in again.", "reconnect_required"],
    [
      "OAuth client registration does not match this authorization server.",
      "registration_mismatch",
    ],
    [
      "The MCP authorization setup is unsupported or changed.",
      "authorization_configuration",
    ],
    ["The OS credential vault is unavailable.", "credential_vault"],
    ["MCP protocol negotiation failed.", "protocol_negotiation"],
    ["MCP protocol negotiation timed out.", "protocol_timeout"],
    [
      "MCP catalog request failed; the catalog is incomplete.",
      "catalogue_request",
    ],
    [
      "MCP discovery timed out; the catalog is incomplete.",
      "catalogue_timeout",
    ],
    [
      "MCP catalog exceeds the 128-tool limit; discovery is incomplete.",
      "catalogue_tool_limit",
    ],
    [
      "MCP catalog exceeds the metadata byte limit; discovery is incomplete.",
      "catalogue_byte_limit",
    ],
    ["MCP account setup timed out.", "account_setup_timeout"],
  ]) {
    if (message.includes(text)) return code;
  }
  return "unclassified";
}
export async function runInstalledGitHub(mode) {
  assert.ok(
    ["--preflight", "--catalogue"].includes(mode),
    "Use --preflight or --catalogue",
  );
  const here = dirname(fileURLToPath(import.meta.url));
  const binary = await realpath(
    join(process.env.LOCALAPPDATA, "Grain GitHub Test", "handy.exe"),
  );
  await mkdir(join(here, ".runs"), { recursive: true });
  const root = await mkdtemp(join(here, ".runs/github-alpha-"));
  const report = {
    runId: randomUUID(),
    evidenceClass: "installed-github-test/live-catalogue",
    commit: "installed-binary",
    binarySha256: await hashFile(binary),
    runnerSha256: await hashFile(fileURLToPath(import.meta.url)),
    repository,
    mode,
    results: [],
    cleanup: { status: "Pending" },
  };
  const exec = promisify(execFile);
  let child, browser, main;
  const invoke = async (command, args = {}, timeout = 30000) => {
    let timer;
    try {
      // A timeout never reissues a command; exit cancels this owned application.
      return await Promise.race([
        main.evaluate(
          ({ command, args }) =>
            window.__TAURI_INTERNALS__.invoke(command, args),
          { command, args },
        ),
        new Promise((_, reject) => {
          timer = setTimeout(
            () =>
              reject(
                new Error(
                  `${command} timed out; do not repeat dispatched work`,
                ),
              ),
            timeout,
          );
        }),
      ]);
    } catch (error) {
      error.diagnostic = { command, code: mcpFailureCode(error) };
      throw error;
    } finally {
      clearTimeout(timer);
    }
  };
  const launch = async () => {
    const active = await exec(
      "powershell.exe",
      [
        "-NoProfile",
        "-Command",
        "@(Get-CimInstance Win32_Process -Filter \"Name='handy.exe'\" | Where-Object { $_.ExecutablePath -ieq $env:GRAIN_GITHUB_ALPHA_BINARY }).Count",
      ],
      {
        windowsHide: true,
        env: { ...process.env, GRAIN_GITHUB_ALPHA_BINARY: binary },
      },
    );
    if (Number(active.stdout.trim()) !== 0)
      throw new Blocked(
        "Close the installed GitHub test app before running this check",
      );
    const listener = createServer();
    await new Promise((done) => listener.listen(0, "127.0.0.1", done));
    const port = listener.address().port;
    await new Promise((done) => listener.close(done));
    child = spawn(binary, [], {
      windowsHide: true,
      stdio: "ignore",
      env: {
        ...process.env,
        WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-address=127.0.0.1 --remote-debugging-port=${port}`,
        WEBVIEW2_USER_DATA_FOLDER: join(root, "webview"),
      },
    });
    (report.hostPids ??= []).push(child.pid);
    browser = await waitFor(
      "Owned GitHub test WebView2",
      async () => {
        if (child.exitCode !== null)
          throw new Error("GitHub test app exited before attachment");
        try {
          return await chromium.connectOverCDP(`http://127.0.0.1:${port}`, {
            timeout: 1000,
          });
        } catch {
          return null;
        }
      },
      { timeoutMs: 30000, intervalMs: 250 },
    );
    main = await waitFor("GitHub test main window", async () => {
      for (const page of browser
        .contexts()
        .flatMap((context) => context.pages())) {
        if (new URL(page.url()).origin !== "http://tauri.localhost") continue;
        if (
          await page.evaluate(
            () =>
              window.__TAURI_INTERNALS__?.metadata?.currentWindow?.label ===
              "main",
          )
        )
          return page;
      }
    });
    assert.equal(await invoke("plugin:app|name"), "Grain GitHub Test");
  };
  const stop = async () => {
    if (!child) return;
    if (main && child.exitCode === null) {
      try {
        await invoke("plugin:process|exit", { code: 0 }, 5000);
      } catch {
        /* normal IPC disconnect on exit */
      }
    }
    if (child.exitCode === null) {
      try {
        await waitFor("Owned GitHub test exit", () => child.exitCode !== null, {
          timeoutMs: 5000,
        });
      } catch {
        child.kill();
        await waitFor(
          "Owned GitHub test termination",
          () => child.exitCode !== null || child.signalCode !== null,
        );
      }
    }
    await browser?.close();
    child = browser = main = null;
  };
  const check = async (id, operation) => {
    console.log(`RUN ${id}`);
    try {
      const evidence = await operation();
      report.results.push({ id, status: "Pass", evidence });
      console.log(`PASS ${id}`);
    } catch (error) {
      report.results.push({
        id,
        status: error instanceof Blocked ? "Blocked" : "Fail",
        error:
          error instanceof Blocked
            ? error.message
            : "Check failed; no automatic retry or acceptance credit",
        diagnostic: error.diagnostic ?? {
          type: error?.name ?? typeof error,
          assertion:
            error?.code === "ERR_ASSERTION"
              ? {
                  operator: error.operator,
                  actual:
                    typeof error.actual === "number" ? error.actual : undefined,
                  expected:
                    typeof error.expected === "number"
                      ? error.expected
                      : undefined,
                }
              : undefined,
          markers: [
            "401",
            "403",
            "404",
            "not found",
            "permission",
            "model",
            "API key",
            "unsupported",
            "connection",
            "timed out",
            "confirmation",
            "README",
          ].filter((marker) =>
            String(error).toLowerCase().includes(marker.toLowerCase()),
          ),
        },
      });
      throw error;
    }
  };
  const preflight = async () => {
    const presets = await invoke("mcp_provider_status");
    const github = presets.find((item) => item.id === "github");
    if (github?.endpoint !== "https://api.githubcopilot.com/mcp/")
      throw new Blocked(
        "This installer does not expose the bundled GitHub preset.",
      );
    if (!github.client_id_configured)
      throw new Blocked(
        "This installer is missing the GitHub OAuth registration.",
      );
    if (!github.connected || !github.enabled)
      throw new Blocked(
        "Connect and enable GitHub from Integrations.",
      );
    return {
      connected: github.connected,
      enabled: github.enabled,
      accountState: github.state,
    };
  };
  try {
    await mkdir(join(root, "webview"));
    await launch();
    await check("github.preset-preflight", preflight);
    if (mode === "--catalogue") {
      await check("github.catalogue", async () => {
        const discovered = await invoke(
          "mcp_test_provider",
          {
            id: "github",
          },
          120000,
        );
        assert.equal(discovered.provider_id, "github");
        assert.equal(discovered.tool_count, discovered.tools.length);
        assert.ok(discovered.tool_count > 0);
        assert.ok(discovered.tools.includes("get_file_contents"));
        assert.ok(discovered.tools.includes("issue_write"));
        const evidence = {
          providerId: "github",
          count: discovered.tool_count,
          tools: discovered.tools,
          digest: discovered.tool_set_digest,
        };
        console.log(JSON.stringify({ catalogue: evidence }));
        return evidence;
      });
    }
  } catch (error) {
    console.error(
      error instanceof Blocked
        ? error.message
        : "GitHub check failed. Inspect its scoped report.",
    );
    process.exitCode = 1;
  } finally {
    try {
      await stop();
      report.cleanup.status = "Pass";
    } catch {
      report.cleanup.status = "Fail";
      process.exitCode = 1;
    }
    // Account tokens and the isolated GitHub test profile are deliberately retained.
    await writeReport(root, report);
    console.log(`Report: ${join(root, "evidence/report.md")}`);
  }
}

if (
  process.argv[1] &&
  resolve(process.argv[1]) === fileURLToPath(import.meta.url)
) {
  assert.ok(process.argv.length === 3, "Pass one mode.");
  await runInstalledGitHub(process.argv[2]);
}
