// Public test signer + owned loopback transport. Never a production credential.
import assert from "node:assert/strict";
import { createServer } from "node:http";
import {
  createPrivateKey,
  createPublicKey,
  sign,
  createHash,
} from "node:crypto";
import { fixturePackage } from "./installation.mjs";

const keyId = Buffer.from("grain-h1");
const privateKey = createPrivateKey({
  key: Buffer.concat([
    Buffer.from("302e020100300506032b657004220420", "hex"),
    Buffer.alloc(32, 71),
  ]),
  format: "der",
  type: "pkcs8",
});
export const STORE_PUBLIC_KEY = Buffer.concat([
  Buffer.from("Ed"),
  keyId,
  createPublicKey(privateKey)
    .export({ format: "der", type: "spki" })
    .subarray(-32),
]).toString("base64");
export function signStoreBytes(bytes, corrupt = false) {
  const signature = sign(
    null,
    createHash("blake2b512").update(bytes).digest(),
    privateKey,
  );
  if (corrupt) signature[0] ^= 1;
  const comment = "harness fixture";
  return `untrusted comment: PUBLIC TEST KEY\n${Buffer.concat([Buffer.from("ED"), keyId, signature]).toString("base64")}\ntrusted comment: ${comment}\n${sign(null, Buffer.concat([signature, Buffer.from(comment)]), privateKey).toString("base64")}\n`;
}

export async function startStore(here) {
  const sockets = new Set(),
    held = new Set(),
    journal = [];
  let version = 100,
    pack,
    bytes,
    hash,
    index,
    offline = false,
    badSignature = false,
    corruptBlob = false,
    holdPath = null;
  function respond(response, path) {
    if (response.destroyed) return;
    let body,
      code = 200;
    if (offline) code = 503;
    else if (path === "/index.json") body = index;
    else if (path === "/index.json.minisig")
      body = Buffer.from(signStoreBytes(index, badSignature));
    else if (path === `/blob/${hash}.grainpack`) {
      body = Buffer.from(bytes);
      if (corruptBlob) body[body.length - 2] ^= 1;
    } else code = 404;
    response.writeHead(code, {
      "Content-Length": body?.length ?? 0,
      Connection: "close",
    });
    response.end(body);
  }
  const server = createServer((request, response) => {
    if (journal.length >= 2048 || request.method !== "GET") {
      response.writeHead(400);
      response.end();
      return;
    }
    const path = request.url;
    journal.push({ path, sequence: journal.length + 1 });
    if (
      path === holdPath ||
      (holdPath === "blob" && path.startsWith("/blob/"))
    ) {
      holdPath = null;
      const item = { response, path };
      held.add(item);
      response.once("close", () => held.delete(item));
    } else respond(response, path);
  });
  server.on("connection", (socket) => {
    sockets.add(socket);
    socket.once("close", () => sockets.delete(socket));
  });
  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolve);
  });
  return {
    port: server.address().port,
    journal,
    get heldCount() {
      return held.size;
    },
    get hash() {
      return hash;
    },
    async configure(revision = "store-one", packageVersion = "0.1.0") {
      assert.equal(held.size, 0, "Previous store request still held");
      pack = await fixturePackage(here, revision);
      pack.manifest.version = packageVersion;
      bytes = Buffer.from(JSON.stringify(pack));
      hash = createHash("sha256").update(bytes).digest("hex");
      index = Buffer.from(
        JSON.stringify({
          spec: 1,
          version: ++version,
          expires: new Date(Date.now() + 7 * 86400000).toISOString(),
          entries: [
            {
              id: pack.manifest.id,
              name: pack.manifest.name,
              version: packageVersion,
              tier: "scripted",
              trust: "verified",
              capabilities: [],
              sha256: hash,
              size: bytes.length,
              min_grain_api: "0.0.0",
            },
          ],
        }),
      );
      offline = badSignature = corruptBlob = false;
      holdPath = null;
    },
    offline(value = true) {
      offline = value;
    },
    badSignature(value = true) {
      badSignature = value;
    },
    corruptBlob(value = true) {
      corruptBlob = value;
    },
    hold(path) {
      assert.equal(held.size, 0);
      holdPath = path;
    },
    release() {
      for (const item of held) {
        held.delete(item);
        respond(item.response, item.path);
      }
    },
    async close() {
      for (const socket of sockets) socket.destroy();
      held.clear();
      await new Promise((resolve) => server.close(resolve));
    },
  };
}
