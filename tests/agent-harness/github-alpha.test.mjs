import { test } from "node:test";
import assert from "node:assert/strict";
import { approvedGitHubRead } from "./github-alpha.mjs";

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
