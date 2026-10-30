#!/usr/bin/env node
// Maintainer deployment check. No app setting, vault or hosting mutation.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { isIP } from "node:net";
import { pathToFileURL } from "node:url";

export const GRAIN_MCP_REDIRECT = "http://127.0.0.1:31938/mcp/oauth/callback";
export const MAX_CLIENT_DOCUMENT_BYTES = 8192;

export function validateClientDocument(raw, expectedUrl) {
  const url = new URL(expectedUrl);
  assert.equal(url.protocol, "https:", "Client identity requires HTTPS");
  assert.equal(
    url.href,
    expectedUrl,
    "Client identity must be the exact canonical URL",
  );
  assert.ok(url.pathname !== "/", "Client identity requires a document path");
  assert.ok(
    !url.username &&
      !url.password &&
      !expectedUrl.includes("?") &&
      !expectedUrl.includes("#"),
    "Client identity cannot contain credentials, query or fragment",
  );
  assert.ok(
    !isIP(url.hostname.replace(/^\[|\]$/g, "")) &&
      !/(^localhost$|\.(localhost|local|invalid)$)/i.test(url.hostname),
    "Client identity requires permanent public hosting",
  );
  assert.ok(
    Buffer.byteLength(raw, "utf8") <= MAX_CLIENT_DOCUMENT_BYTES,
    "Client document exceeds the deployment bound",
  );
  const value = JSON.parse(raw);
  assert.equal(
    value.client_id,
    expectedUrl,
    "Document identity does not match its URL",
  );
  assert.equal(value.client_name, "Grain");
  assert.equal(value.application_type, "native");
  assert.deepEqual(
    value.redirect_uris,
    [GRAIN_MCP_REDIRECT],
    "Document redirects do not match Grain's supported callback inventory",
  );
  assert.deepEqual(value.grant_types, ["authorization_code", "refresh_token"]);
  assert.deepEqual(value.response_types, ["code"]);
  assert.equal(value.token_endpoint_auth_method, "none");
  assert.ok(
    Object.keys(value).every((key) =>
      [
        "client_id",
        "client_name",
        "application_type",
        "redirect_uris",
        "grant_types",
        "response_types",
        "token_endpoint_auth_method",
      ].includes(key),
    ),
    "Unexpected metadata field; review changes before publishing",
  );
  return value;
}

export async function verifyLiveDocument(expectedUrl) {
  // Validate the candidate URL before making a request; no redirects, credentials
  // or cached bearer material are used by this independent publication check.
  const proposed = {
    client_id: expectedUrl,
    client_name: "Grain",
    application_type: "native",
    redirect_uris: [GRAIN_MCP_REDIRECT],
    grant_types: ["authorization_code", "refresh_token"],
    response_types: ["code"],
    token_endpoint_auth_method: "none",
  };
  validateClientDocument(JSON.stringify(proposed), expectedUrl);
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), 10000);
  let reader;
  try {
    const response = await fetch(expectedUrl, {
      redirect: "manual",
      signal: controller.signal,
      headers: { accept: "application/json" },
    });
    assert.equal(
      response.status,
      200,
      "Public client document must return 200 without a redirect",
    );
    assert.equal(
      response.headers.get("content-type")?.split(";")[0].trim().toLowerCase(),
      "application/json",
    );
    reader = response.body.getReader();
    const chunks = [];
    let bytes = 0;
    for (;;) {
      const next = await reader.read();
      if (next.done) break;
      bytes += next.value.byteLength;
      assert.ok(
        bytes <= MAX_CLIENT_DOCUMENT_BYTES,
        "Client document exceeds the deployment bound",
      );
      chunks.push(next.value);
    }
    validateClientDocument(
      new TextDecoder("utf-8", { fatal: true }).decode(Buffer.concat(chunks)),
      expectedUrl,
    );
    return { status: "Pass", bytes, redirect: GRAIN_MCP_REDIRECT };
  } finally {
    controller.abort();
    clearTimeout(timer);
    await reader?.cancel().catch(() => {});
  }
}

if (
  process.argv[1] &&
  import.meta.url === pathToFileURL(process.argv[1]).href
) {
  try {
    const args = process.argv.slice(2);
    assert.ok(
      (args.length === 3 && args[1] === "--file") ||
        (args.length === 2 && args[1] === "--live"),
      "Usage: node scripts/check-mcp-client-metadata.mjs HTTPS_URL --file DOCUMENT.json | HTTPS_URL --live",
    );
    if (args[1] === "--file") {
      validateClientDocument(await readFile(args[2], "utf8"), args[0]);
      console.log(
        "Pass: local client document matches Grain's callback inventory. Public deployment is not verified.",
      );
    } else {
      console.log(JSON.stringify(await verifyLiveDocument(args[0])));
    }
  } catch (error) {
    console.error("Client metadata check failed: " + error.message);
    process.exitCode = 1;
  }
}
