import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import YAML from "yaml";
import { mcpFailureCode, runInstalledGitHub } from "./github-alpha.mjs";

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

test("installed GitHub acceptance refuses live Agent modes before launching", async () => {
  for (const mode of ["--read", "--workflow"]) {
    await assert.rejects(
      runInstalledGitHub(mode),
      /Use --preflight or --catalogue/,
    );
  }
});
