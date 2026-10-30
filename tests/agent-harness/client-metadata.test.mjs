import assert from "node:assert/strict";
import test from "node:test";
import { readFile } from "node:fs/promises";
import {
  validateClientDocument,
  GRAIN_MCP_REDIRECT,
} from "../../scripts/check-mcp-client-metadata.mjs";

const url = "https://grain.example/oauth/client.json";
const document = {
  client_id: url,
  client_name: "Grain",
  application_type: "native",
  redirect_uris: [GRAIN_MCP_REDIRECT],
  grant_types: ["authorization_code", "refresh_token"],
  response_types: ["code"],
  token_endpoint_auth_method: "none",
};

test("public metadata checker rejects URL substitution, widened callbacks and client secrets", () => {
  assert.deepEqual(
    validateClientDocument(JSON.stringify(document), url),
    document,
  );
  for (const change of [
    { client_id: url + "/" },
    { application_type: "web" },
    { redirect_uris: ["http://localhost:31938/mcp/oauth/callback"] },
    {
      redirect_uris: [
        GRAIN_MCP_REDIRECT,
        "http://127.0.0.1:31939/mcp/oauth/callback",
      ],
    },
    { token_endpoint_auth_method: "client_secret_post" },
    { client_secret: "never-publish" },
    { grant_types: ["authorization_code"] },
  ])
    assert.throws(() =>
      validateClientDocument(JSON.stringify({ ...document, ...change }), url),
    );
  for (const candidate of [
    "http://grain.example/oauth/client.json",
    "https://grain.example/",
    "https://grain.example/oauth/client.json#fragment",
    "https://name:secret@grain.example/oauth/client.json",
    "https://127.0.0.1/oauth/client.json",
    "https://localhost/oauth/client.json",
  ])
    assert.throws(() =>
      validateClientDocument(
        JSON.stringify({ ...document, client_id: candidate }),
        candidate,
      ),
    );
  assert.throws(() => validateClientDocument(" ".repeat(8193), url), /bound/);
});

test("publication inventory matches the actual backend callback; experimental hosting stays off", async () => {
  const source = await readFile(
    new URL("../../src-tauri/src/grain_mcp.rs", import.meta.url),
    "utf8",
  );
  const address = source.match(/const CALLBACK_ADDR: &str = "([^"]+)";/)?.[1];
  const path = source.match(/const CALLBACK_PATH: &str = "([^"]+)";/)?.[1];
  assert.equal(`http://${address}${path}`, GRAIN_MCP_REDIRECT);
  assert.match(source, /const CLIENT_METADATA_URL: Option<&str> = None;/);
});

test("CIMD fault is isolated to its owned registration scenario", async () => {
  const { promisify } = await import("node:util");
  const { execFile } = await import("node:child_process");
  await assert.rejects(
    promisify(execFile)(process.execPath, [
      "tests/agent-harness/run.mjs",
      "--scenario",
      "mcp.auth-fixture",
      "--fault",
      "missing-mcp-cimd",
    ]),
    (error) =>
      error.stderr.includes("requires --scenario mcp.auth-client-metadata"),
  );
});
