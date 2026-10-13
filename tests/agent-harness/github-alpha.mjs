// Installed-alpha acceptance through real Tauri IPC/WebView2. No Computer Use,
// screenshots, mock app, profile copying, token extraction or model relay.
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

export const repository = "Punit-Dethe/grain-github-alpha-test";
export function approvedGitHubRead(confirm) {
  if (
    !confirm ||
    typeof confirm.token !== "string" ||
    !confirm.token ||
    !Array.isArray(confirm.details) ||
    !/^(get_file_contents|Get file or directory contents)$/i.test(
      confirm.title ?? "",
    )
  )
    return false;
  const fields = new Map(
    confirm.details.map(({ label, value }) => [label, value]),
  );
  return (
    fields.size === confirm.details.length &&
    [...fields.keys()].every((key) =>
      ["owner", "repo", "path", "ref"].includes(key),
    ) &&
    fields.get("owner") === "Punit-Dethe" &&
    fields.get("repo") === "grain-github-alpha-test" &&
    ["README.md", "/README.md"].includes(fields.get("path"))
  );
}

export async function runInstalledGitHub(mode) {
  assert.ok(
    ["--preflight", "--read"].includes(mode),
    "Use --preflight or --read; no writes are enabled",
  );
  const here = dirname(fileURLToPath(import.meta.url));
  const root = await mkdtemp(join(here, ".runs/github-alpha-"));
  const binary = await realpath(
    join(process.env.LOCALAPPDATA, "Grain Extensions Alpha", "handy.exe"),
  );
  const report = {
    runId: randomUUID(),
    evidenceClass: "installed-alpha/real-agent-ipc/live-github",
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
        "Close the installed extension alpha before running this check",
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
      "Owned alpha WebView2",
      async () => {
        if (child.exitCode !== null)
          throw new Error("Alpha exited before attachment");
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
    main = await waitFor("Alpha main window", async () => {
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
    assert.equal(await invoke("plugin:app|name"), "Grain Extensions Alpha");
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
        await waitFor("Owned alpha exit", () => child.exitCode !== null, {
          timeoutMs: 5000,
        });
      } catch {
        child.kill();
        await waitFor(
          "Owned alpha termination",
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
  let connection;
  const preflight = async () => {
    const settings = await main.evaluate(async () => {
      const value = await window.__TAURI_INTERNALS__.invoke("get_app_settings");
      return {
        agentEnabled: value.agent_enabled,
        developerMode: value.extension_developer_mode,
        modelSelected:
          !!value.post_process_models?.[value.post_process_provider_id],
        model: value.post_process_models?.[value.post_process_provider_id],
      };
    });
    if (
      !settings.agentEnabled ||
      !settings.developerMode ||
      !settings.modelSelected
    )
      throw new Blocked(
        "Enable Agent, select its model and enable extension Developer Mode in the alpha",
      );
    const connections = await invoke("mcp_connections_list");
    const matches = connections.filter(
      (row) =>
        row.source === "store" &&
        row.extensionId === "com.grain.github" &&
        row.url === "https://api.githubcopilot.com/mcp/",
    );
    if (matches.length !== 1)
      throw new Blocked("Install the verified GitHub extension in the alpha");
    connection = matches[0];
    const status = await invoke("mcp_connection_status", {
      id: connection.id,
      expectedRevision: connection.revision,
    });
    if (!status.connected || !status.enabled)
      throw new Blocked(
        "Sign in and enable the installed GitHub extension in Grain",
      );
    // Keep unrelated tools out of this provider-specific acceptance batch.
    const enabled = connections.filter((row) => row.state === "enabled");
    const native = await invoke("extensions_overview");
    if (
      enabled.some((row) => row.id !== connection.id) ||
      native.some((card) => card.enabled && card.trust !== "core")
    )
      throw new Blocked(
        "Disable other extension tools for this focused GitHub check",
      );
    return {
      ...settings,
      connected: status.connected,
      enabled: status.enabled,
      accountState: status.state,
    };
  };
  const read = async (expected) => {
    let reply = await invoke(
      "agent_run",
      {
        messages: [
          {
            role: "user",
            content: `Use only the installed GitHub extension's get_file_contents tool with owner=Punit-Dethe, repo=grain-github-alpha-test and path=README.md on its default branch. The repository is already known: do not search repositories or fetch account information. Return the complete README verbatim. Do not create or modify anything. Do not use other extensions. If access is denied or the file cannot be read, report that error and stop.`,
          },
        ],
        context: null,
      },
      300000,
    );
    let approvals = 0;
    while (reply.confirm_action) {
      const confirm = reply.confirm_action;
      if (approvals >= 2 || !approvedGitHubRead(confirm)) {
        console.log(
          JSON.stringify({
            refusedConfirmation: {
              title: confirm.title,
              labels: confirm.details.map((field) => field.label),
              sideEffect: confirm.side_effect,
              approvedReads: approvals,
              readmeAlreadyVerified: reply.text.includes(expected.trim()),
              accessErrorObserved:
                /not found|404|403|forbidden|permission|access denied/i.test(
                  reply.text,
                ),
            },
          }),
        );
        await invoke(
          "agent_confirm_action",
          { token: confirm.token, approve: false },
          300000,
        );
        const error = new Error("GitHub read requested an unapproved action");
        error.diagnostic = {
          approvedReads: approvals,
          refusedTitle: confirm.title,
          accessErrorObserved:
            /not found|404|403|forbidden|permission|access denied/i.test(
              reply.text,
            ),
        };
        throw error;
      }
      approvals++;
      reply = await invoke(
        "agent_confirm_action",
        { token: confirm.token, approve: true },
        300000,
      );
    }
    console.log(
      JSON.stringify({
        readResult: {
          approvedReads: approvals,
          replyBytes: Buffer.byteLength(reply.text),
          readmeVerified: reply.text.includes(expected.trim()),
          accessErrorObserved:
            /not found|404|403|forbidden|permission|access denied/i.test(
              reply.text,
            ),
        },
      }),
    );
    assert.ok(
      approvals > 0,
      "Require a real host-confirmed GitHub read, not a model-only answer",
    );
    if (!reply.text.includes(expected.trim())) {
      const error = new Error(
        "Actual test README must match independently observed GitHub bytes",
      );
      error.diagnostic = {
        approvedReads: approvals,
        replyBytes: Buffer.byteLength(reply.text),
        markers: [
          "401",
          "403",
          "404",
          "not found",
          "permission",
          "access denied",
          "authentication",
          "GitHub",
          "error",
          "failed",
          "budget",
          "unknown",
        ].filter((marker) =>
          reply.text.toLowerCase().includes(marker.toLowerCase()),
        ),
      };
      console.log(JSON.stringify({ failedRead: error.diagnostic }));
      throw error;
    }
    return {
      approvedReads: approvals,
      readmeVerified: true,
      connectionRevision: connection.revision,
    };
  };
  try {
    await mkdir(join(root, "webview"));
    await launch();
    await check("github.installed-preflight", preflight);
    if (mode === "--read") {
      const result = JSON.parse(
        (
          await exec("gh", ["api", `repos/${repository}/contents/README.md`], {
            windowsHide: true,
            maxBuffer: 128 * 1024,
          })
        ).stdout,
      );
      assert.equal(result.encoding, "base64");
      const expected = Buffer.from(result.content, "base64").toString("utf8");
      assert.ok(expected.length > 0 && expected.length < 16384);
      await check("github.agent-read", () => read(expected));
      await stop();
      await launch();
      await check("github.restart-account", preflight);
      await check("github.agent-read-after-restart", () => read(expected));
    }
  } catch (error) {
    console.error(
      error instanceof Blocked
        ? error.message
        : "GitHub check failed; inspect its scoped report. Do not replay a write.",
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
    // Account tokens and the ordinary alpha profile are deliberately retained.
    await writeReport(root, report);
    console.log(`Report: ${join(root, "evidence/report.md")}`);
  }
}

if (
  process.argv[1] &&
  resolve(process.argv[1]) === fileURLToPath(import.meta.url)
)
  await runInstalledGitHub(process.argv[2]);
