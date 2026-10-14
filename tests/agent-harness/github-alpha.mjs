// Installed-alpha acceptance through real Tauri IPC/WebView2. No Computer Use,
// screenshots, mock app, profile copying, token extraction or model relay.
import assert from "node:assert/strict";
import { spawn, execFile } from "node:child_process";
import { promisify } from "node:util";
import { createServer } from "node:net";
import {
  mkdtemp,
  mkdir,
  realpath,
  readdir,
  stat,
  open,
} from "node:fs/promises";
import { join, dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { randomUUID } from "node:crypto";
import { chromium } from "@playwright/test";
import { waitFor, writeReport, Blocked } from "./support.mjs";
import { hashFile } from "./stamp.mjs";

export const repository = "Punit-Dethe/grain-github-alpha-test";
export function approvedGitHubAction(confirm, titles, required, optional = {}) {
  if (
    required.owner !== "Punit-Dethe" ||
    required.repo !== "grain-github-alpha-test" ||
    !confirm?.token ||
    typeof confirm.token !== "string" ||
    !titles.includes(confirm.title) ||
    !Array.isArray(confirm.details) ||
    !confirm.details.every(
      (field) =>
        field &&
        typeof field.label === "string" &&
        typeof field.value === "string",
    )
  )
    return false;
  const fields = new Map(
    confirm.details.map((field) => [field.label, field.value]),
  );
  return (
    fields.size === confirm.details.length &&
    Object.entries(required).every(
      ([key, value]) => fields.get(key) === value,
    ) &&
    [...fields].every(
      ([key, value]) =>
        Object.hasOwn(required, key) ||
        (Object.hasOwn(optional, key) && optional[key] === value),
    )
  );
}
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

export async function runInstalledGitHub(mode, updateVersion) {
  assert.ok(
    ["--preflight", "--catalogue", "--read", "--workflow"].includes(mode),
    "Use --preflight, --catalogue, --read or --workflow",
  );
  assert.ok(
    updateVersion === undefined || /^\d+\.\d+\.\d+$/.test(updateVersion),
    "An explicit store update requires a release version",
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
  let previousLogLevel;
  let logDirectory;
  const logOffsets = new Map();
  const trace = async () => {
    const events = [];
    if (!logDirectory) return events;
    for (const name of (await readdir(logDirectory)).filter((name) =>
      /^handy.*\.log$/.test(name),
    )) {
      const path = join(logDirectory, name);
      const size = (await stat(path)).size;
      const offset = Math.max(logOffsets.get(name) ?? 0, size - 128 * 1024);
      logOffsets.set(name, size);
      if (offset >= size) continue;
      const handle = await open(path, "r");
      try {
        const buffer = Buffer.alloc(size - offset);
        await handle.read(buffer, 0, buffer.length, offset);
        const completeBytes = buffer.lastIndexOf(10) + 1;
        for (const line of buffer
          .subarray(0, completeBytes)
          .toString("utf8")
          .split(/\r?\n/)) {
          const marker = "[GRAIN] agent-tool-trace ";
          const start = line.indexOf(marker);
          if (start < 0) continue;
          const event = JSON.parse(line.slice(start + marker.length));
          if (["offered", "selected", "loaded"].includes(event.phase))
            events.push(event);
        }
        logOffsets.set(name, offset + completeBytes);
      } finally {
        await handle.close();
      }
    }
    return events;
  };
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
      const diagnostic = report.results.at(-1).diagnostic;
      if (logDirectory && !diagnostic.trace) diagnostic.trace = await trace();
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
    const catalogueIds = (await invoke("mcp_provider_status")).map(
      (item) => item.id,
    );
    const enabledCatalogueIds = await main.evaluate(async (ids) => {
      const settings =
        await window.__TAURI_INTERNALS__.invoke("get_app_settings");
      return settings.mcp_enabled_providers.filter((id) => ids.includes(id));
    }, catalogueIds);
    if (enabledCatalogueIds.length)
      throw new Blocked(
        "Disable developer-catalogue MCP entries before an installed-extension check",
      );
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
      enabledCatalogueCount: enabledCatalogueIds.length,
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
      trace: await trace(),
    };
  };
  const action = async (
    prompt,
    required,
    titles,
    optional = {},
    readApproval,
  ) => {
    let writes = 0,
      reads = 0;
    let reply = await invoke(
      "agent_run",
      {
        messages: [
          {
            role: "user",
            content: `${prompt} The repository is already known. Do not search repositories or fetch account information. Use only the installed GitHub extension. Perform this operation once; do not repeat it if the result is uncertain. After the requested action, summarize its result and stop.`,
          },
        ],
        context: null,
      },
      300000,
    );
    while (reply.confirm_action) {
      const confirm = reply.confirm_action;
      const write =
        writes === 0 &&
        approvedGitHubAction(confirm, titles, required, optional);
      const read =
        reads === 0 &&
        readApproval &&
        approvedGitHubAction(
          confirm,
          readApproval.titles,
          readApproval.required,
          readApproval.optional,
        );
      if (!write && !read) {
        console.log(
          JSON.stringify({
            refusedAction: {
              title: confirm.title,
              labels: confirm.details?.map((field) => field.label),
            },
          }),
        );
        await invoke(
          "agent_confirm_action",
          { token: confirm.token, approve: false },
          300000,
        );
        const error = new Error(
          "Workflow requested an action outside its exact test scope",
        );
        error.diagnostic = {
          approvedWrites: writes,
          approvedReads: reads,
          refusedTitle: confirm.title,
          fieldLabels: confirm.details?.map((field) => field.label),
          trace: await trace(),
        };
        throw error;
      }
      if (write) writes++;
      else reads++;
      reply = await invoke(
        "agent_confirm_action",
        { token: confirm.token, approve: true },
        300000,
      );
    }
    if (writes !== 1) {
      const error = new Error(
        "Require one host-approved action, not a model claim",
      );
      error.diagnostic = {
        approvedWrites: writes,
        approvedReads: reads,
        replyBytes: Buffer.byteLength(reply.text),
        markers: [
          "401",
          "403",
          "404",
          "permission",
          "not found",
          "budget",
          "unknown",
          "error",
          "failed",
          "confirmation",
        ].filter((marker) => reply.text.toLowerCase().includes(marker)),
      };
      throw error;
    }
    return {
      approvedWrites: writes,
      approvedReads: reads,
      replyBytes: Buffer.byteLength(reply.text),
    };
  };
  try {
    await mkdir(join(root, "webview"));
    await launch();
    await check("github.installed-preflight", preflight);
    if (updateVersion) {
      await check("github.package-update", async () => {
        const previous = connection;
        assert.notEqual(
          previous.version,
          updateVersion,
          "Require an actual update",
        );
        try {
          await invoke("store_browse", {}, 120000);
          await invoke(
            "store_mcp_update",
            {
              connectionId: previous.id,
              expectedRevision: previous.revision,
              version: updateVersion,
            },
            120000,
          );
        } finally {
          await invoke("store_close");
        }
        const updated = (await invoke("mcp_connections_list")).find(
          (row) => row.id === previous.id,
        );
        assert.ok(updated, "Update must retain the connection");
        assert.equal(updated.version, updateVersion);
        assert.notEqual(updated.revision, previous.revision);
        const account = await invoke("mcp_connection_status", {
          id: updated.id,
          expectedRevision: updated.revision,
        });
        assert.equal(account.connected, true, "Update must retain sign-in");
        assert.equal(
          account.enabled,
          false,
          "Update requires explicit enablement",
        );
        await invoke("mcp_connection_set_enabled", {
          id: updated.id,
          expectedRevision: updated.revision,
          enabled: true,
        });
        const status = await preflight();
        assert.equal(connection.id, previous.id);
        assert.equal(connection.version, updateVersion);
        assert.notEqual(connection.revision, previous.revision);
        assert.equal(connection.headers["x-mcp-toolsets"], "all");
        return {
          ...status,
          previousVersion: previous.version,
          version: connection.version,
          disabledBeforeExplicitEnablement: true,
        };
      });
    }
    if (mode === "--workflow" || mode === "--read") {
      previousLogLevel = await main.evaluate(
        async () =>
          (await window.__TAURI_INTERNALS__.invoke("get_app_settings"))
            .log_level,
      );
      assert.ok(
        ["trace", "debug", "info", "warn", "error"].includes(previousLogLevel),
        "Require a restorable log level before changing it",
      );
      await invoke("set_log_level", { level: "info" });
      logDirectory = await invoke("get_log_dir_path");
      for (const name of (await readdir(logDirectory)).filter((name) =>
        /^handy.*\.log$/.test(name),
      ))
        logOffsets.set(name, (await stat(join(logDirectory, name))).size);
    }
    if (mode === "--workflow") {
      const api = async (path) =>
        JSON.parse(
          (
            await exec("gh", ["api", `repos/${repository}/${path}`], {
              windowsHide: true,
              maxBuffer: 128 * 1024,
            })
          ).stdout,
        );
      const marker = report.runId.slice(0, 8);
      report.testObjects = {
        repository,
        marker,
        branch: `grain-check-${marker}`,
        path: "grain-agent-check.txt",
      };
      const title = `Grain Agent check ${marker}`;
      const body = `Disposable Grain acceptance ${marker}.`;
      const issueTitles = [
        "issue_write",
        "Create or update issue/pull request",
      ];
      const scope = { owner: "Punit-Dethe", repo: "grain-github-alpha-test" };
      const emptyArrays = { labels: "[]", assignees: "[]" };
      let number;
      await check("github.issue-create", async () => {
        let result, failure;
        try {
          result = await action(
            `Create an issue in ${repository} with exactly title "${title}" and body "${body}". Do not add labels, assignees or other fields.`,
            { ...scope, method: "create", title, body },
            issueTitles,
            emptyArrays,
          );
        } catch (error) {
          failure = error;
        }
        const matches = (await api("issues?state=all&per_page=100")).filter(
          (issue) => issue.title === title && !issue.pull_request,
        );
        if (failure) {
          failure.diagnostic = {
            ...failure.diagnostic,
            matchingIssues: matches.length,
            effectObserved: matches.length > 0,
          };
          throw failure;
        }
        assert.equal(
          matches.length,
          1,
          "Independently require one issue; never replay an uncertain write",
        );
        assert.equal(matches[0].body, body);
        number = matches[0].number;
        report.testObjects.issueNumber = number;
        return {
          ...result,
          issueNumber: number,
          effectVerified: true,
          trace: await trace(),
        };
      });
      await check("github.issue-update-close", async () => {
        const updated = `Updated Grain acceptance ${marker}.`;
        const result = await action(
          `In ${repository}, update issue #${number} to exactly body "${updated}" and close it as completed. Keep its title, labels and assignees unchanged.`,
          {
            ...scope,
            method: "update",
            issue_number: String(number),
            body: updated,
            state: "closed",
          },
          issueTitles,
          { title, state_reason: "completed" },
          {
            titles: ["issue_read", "Get issue information"],
            required: { ...scope, method: "get", issue_number: String(number) },
          },
        );
        const issue = await api(`issues/${number}`);
        assert.equal(issue.body, updated);
        assert.equal(issue.state, "closed");
        assert.equal(issue.title, title);
        return { ...result, effectVerified: true, trace: await trace() };
      });
      const branch = `grain-check-${marker}`;
      const mainSha = (await api("git/ref/heads/main")).object.sha;
      await check("github.branch-create", async () => {
        const result = await action(
          `Create branch "${branch}" from main in ${repository}. Do not modify files or create a pull request.`,
          { ...scope, branch },
          ["create_branch", "Create branch"],
          { from_branch: "main" },
        );
        const ref = await api(`git/ref/heads/${branch}`);
        assert.equal(ref.ref, `refs/heads/${branch}`);
        assert.equal(ref.object.sha, mainSha);
        report.testObjects.branch = branch;
        return { ...result, effectVerified: true, trace: await trace() };
      });
      await check("github.file-create", async () => {
        const path = "grain-agent-check.txt";
        const content = `Grain check ${marker}\n`;
        const message = "Add Grain acceptance file";
        const result = await action(
          `Create ${path} on the existing branch "${branch}" in ${repository}. The preceding check already created and verified this branch; do not create it again. Its exact text is "Grain check ${marker}" followed by one newline. Commit message must be "${message}". Do not change main or any other file.`,
          {
            ...scope,
            branch,
            path,
            message,
            content: Buffer.from(content).toString("base64"),
          },
          ["create_or_update_file", "Create or update file"],
        );
        const file = await api(`contents/${path}?ref=${branch}`);
        assert.equal(
          Buffer.from(file.content, "base64").toString("utf8"),
          content,
        );
        assert.equal(
          (await api("git/ref/heads/main")).object.sha,
          mainSha,
          "The default branch must stay unchanged",
        );
        report.testObjects.path = path;
        return { ...result, effectVerified: true, trace: await trace() };
      });
      await check("github.model-exposure-observed", async () => {
        const events = report.results.flatMap(
          (result) => result.evidence?.trace ?? [],
        );
        const offered = events.filter((event) => event.phase === "offered");
        if (!offered.length)
          throw new Blocked(
            "This installed binary predates tool tracing; do not infer real model exposure from source tests",
          );
        assert.ok(
          offered.some(
            (event) =>
              event.count === 2 &&
              event.names.includes("search_tools") &&
              event.names.includes("load_extension"),
          ),
        );
        assert.ok(offered.some((event) => event.count > 2));
        assert.ok(
          events.some(
            (event) =>
              event.phase === "loaded" && Object.keys(event.tools).length > 0,
          ),
        );
        return {
          initialSchemaCount: offered[0].count,
          schemaCounts: offered.map((event) => event.count),
          selected: events
            .filter((event) => event.phase === "selected")
            .map((event) => event.names),
        };
      });
    }
    if (mode === "--catalogue") {
      await check("github.catalogue", async () => {
        const discovered = await invoke(
          "mcp_test_provider",
          {
            id: `configured-${connection.id}`,
          },
          120000,
        );
        assert.equal(discovered.provider_id, `configured-${connection.id}`);
        assert.equal(discovered.tool_count, discovered.tools.length);
        assert.ok(discovered.tool_count > 0);
        assert.ok(discovered.tools.includes("get_file_contents"));
        assert.ok(discovered.tools.includes("issue_write"));
        const evidence = {
          connectionRevision: connection.revision,
          count: discovered.tool_count,
          tools: discovered.tools,
          digest: discovered.tool_set_digest,
        };
        console.log(JSON.stringify({ catalogue: evidence }));
        return evidence;
      });
    }
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
    let restorationFailed = false;
    if (previousLogLevel) {
      try {
        await invoke("set_log_level", { level: previousLogLevel });
        report.logLevelRestored = true;
      } catch {
        restorationFailed = true;
        report.logLevelRestored = false;
      }
    }
    try {
      await stop();
      report.cleanup.status = restorationFailed ? "Fail" : "Pass";
      if (restorationFailed) process.exitCode = 1;
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
) {
  const update = process.argv[3]?.match(/^--update-version=(\d+\.\d+\.\d+)$/);
  assert.ok(
    process.argv.length <= 4 && (!process.argv[3] || update),
    "Unexpected arguments",
  );
  await runInstalledGitHub(process.argv[2], update?.[1]);
}
