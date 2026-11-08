import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import YAML from "yaml";
import {
  approvedGitHubRead,
  approvedGitHubAction,
  mcpFailureCode,
} from "./github-alpha.mjs";

test("distribution builds include GitHub registration while PR builds remain opted out", () => {
  const workflow = (name) =>
    YAML.parse(
      readFileSync(
        new URL(`../../.github/workflows/${name}.yml`, import.meta.url),
        "utf8",
      ),
    );
  for (const [file, job] of [
    ["main-build", "build"],
    ["release", "publish-tauri"],
    ["build-test", "build-test"],
    ["build-test", "windows-beta"],
  ]) {
    assert.equal(
      workflow(file).jobs[job].with["github-extension-auth"],
      true,
      `${file}/${job}`,
    );
  }
  const build = workflow("build");
  assert.equal(
    build.on.workflow_call.inputs["github-extension-auth"].default,
    false,
  );
  assert.equal(
    workflow("pr-test-build").jobs["build-test"].with["github-extension-auth"],
    undefined,
  );
  const steps = build.jobs.build.steps;
  const validation = steps.find(
    (step) => step.name === "Validate GitHub extension registration",
  );
  const compile = steps.find((step) => step.name === "Build with Tauri");
  assert.match(validation.if, /inputs\.github-extension-auth/);
  for (const name of [
    "GRAIN_GITHUB_OAUTH_CLIENT_ID",
    "GRAIN_GITHUB_OAUTH_CLIENT_SECRET",
  ]) {
    assert.match(compile.env[name], /inputs\.github-extension-auth/);
    assert.ok(compile.env[name].includes(`secrets.${name}`));
  }
});

test("MCP failure diagnostics distinguish host failures without retaining provider secrets", () => {
  assert.equal(
    mcpFailureCode(
      "page.evaluate: MCP protocol negotiation failed. private-token",
    ),
    "protocol_negotiation",
  );
  assert.equal(
    mcpFailureCode(
      "MCP authentication is temporarily unavailable. private-token",
    ),
    "authentication_unavailable",
  );
  assert.equal(mcpFailureCode("provider said private-token"), "unclassified");
});

const confirmation = (changes = {}) => ({
  token: "owned-test-token",
  title: "get_file_contents",
  details: [
    { label: "owner", value: "Punit-Dethe" },
    { label: "repo", value: "grain-github-alpha-test" },
    { label: "path", value: "README.md" },
  ],
  ...changes,
});

test("GitHub workflow approvals reject changed scope, extra arguments, duplicate fields and display truncation", () => {
  const required = {
    owner: "Punit-Dethe",
    repo: "grain-github-alpha-test",
    method: "create",
    title: "Grain test",
    body: "Disposable test.",
  };
  const titles = ["issue_write", "Create or update issue/pull request"];
  const value = {
    token: "test-token",
    title: titles[0],
    details: Object.entries(required).map(([label, value]) => ({
      label,
      value,
    })),
  };
  assert.equal(approvedGitHubAction(value, titles, required), true);
  for (const change of [
    { token: "" },
    { title: "delete_repository" },
    { details: [null] },
    { details: [...value.details, value.details[0]] },
    { details: [...value.details, { label: "constructor", value: "unasked" }] },
    {
      details: [...value.details, { label: "assignees", value: '["someone"]' }],
    },
  ]) {
    assert.equal(
      approvedGitHubAction({ ...value, ...change }, titles, required),
      false,
    );
  }
  for (let index = 0; index < value.details.length; index++) {
    const altered = structuredClone(value);
    altered.details[index].value += "...";
    assert.equal(approvedGitHubAction(altered, titles, required), false);
  }
  assert.equal(
    approvedGitHubAction(value, titles, { ...required, repo: "another-repo" }),
    false,
  );
});
test("installed GitHub read approval is confined to the test README and host confirmation", () => {
  assert.equal(approvedGitHubRead(confirmation()), true);
  assert.equal(
    approvedGitHubRead(
      confirmation({ title: "Get file or directory contents" }),
    ),
    true,
  );
  for (const change of [
    { token: "" },
    { title: "delete_file" },
    { details: [] },
    { details: null },
  ])
    assert.equal(approvedGitHubRead(confirmation(change)), false);
  for (let field = 0; field < 3; field++) {
    const value = confirmation();
    value.details[field].value = "unapproved";
    assert.equal(approvedGitHubRead(value), false);
  }
  const duplicated = confirmation();
  duplicated.details.push(duplicated.details[0]);
  assert.equal(approvedGitHubRead(duplicated), false);
  const unexpected = confirmation();
  unexpected.details.push({ label: "content", value: "must not write" });
  assert.equal(approvedGitHubRead(unexpected), false);
});

test("GitHub MCP file approval requires literal content, not REST base64 encoding", () => {
  const required = {
    owner: "Punit-Dethe",
    repo: "grain-github-alpha-test",
    branch: "grain-check-test",
    path: "grain-agent-check.txt",
    message: "Add Grain acceptance file",
    content: "Grain check test\n",
  };
  const confirm = {
    token: "test-token",
    title: "Create or update file",
    details: Object.entries(required).map(([label, value]) => ({
      label,
      value,
    })),
  };
  assert.equal(approvedGitHubAction(confirm, [confirm.title], required), true);
  const encoded = structuredClone(confirm);
  encoded.details.find((field) => field.label === "content").value =
    Buffer.from(required.content).toString("base64");
  assert.equal(approvedGitHubAction(encoded, [confirm.title], required), false);
});
