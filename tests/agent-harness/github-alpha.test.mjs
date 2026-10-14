import { test } from "node:test";
import assert from "node:assert/strict";
import { approvedGitHubRead, approvedGitHubAction } from "./github-alpha.mjs";

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
