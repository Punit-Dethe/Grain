import { readFile } from "node:fs/promises";
import { join } from "node:path";
import { FIXTURE_ID } from "./model.mjs";
import { waitFor, Blocked } from "./support.mjs";

// Existing production DevControl protocol, not a harness-specific reload API.
export async function developerReload(root, extensionId = FIXTURE_ID) {
  if (![FIXTURE_ID, "com.grain.harness.auth"].includes(extensionId))
    throw new Error("Developer reload requires a fixed harness fixture");
  if (!globalThis.WebSocket)
    throw new Blocked("Node 22+ WebSocket client is required");
  const credential = JSON.parse(
    await readFile(join(root, "data/extension-dev-token.json"), "utf8"),
  );
  if (
    credential.url !== "ws://127.0.0.1:17124" ||
    typeof credential.token !== "string"
  )
    throw new Error(
      "Developer credential does not belong to this isolated host",
    );
  const socket = new WebSocket(credential.url);
  try {
    return await new Promise((resolve, reject) => {
      let welcomed = false;
      const timeout = setTimeout(
        () =>
          fail(new Error("Developer reload exceeded its 10-second deadline")),
        10000,
      );
      function fail(error) {
        clearTimeout(timeout);
        reject(error);
      }
      socket.addEventListener(
        "open",
        () =>
          socket.send(
            JSON.stringify({
              token: credential.token,
              client: "agent-harness",
              grain_api: "1.0",
            }),
          ),
        { once: true },
      );
      socket.addEventListener(
        "error",
        () => fail(new Error("Developer socket failed")),
        { once: true },
      );
      socket.addEventListener(
        "close",
        () =>
          fail(new Error("Developer socket closed before its reload result")),
        { once: true },
      );
      socket.addEventListener("message", ({ data }) => {
        try {
          if (typeof data !== "string" || data.length > 65536)
            throw new Error("Invalid developer frame");
          const frame = JSON.parse(data);
          if (!welcomed) {
            if (frame.grain_api !== "1.0")
              throw new Error("Invalid developer welcome");
            welcomed = true;
            socket.send(
              JSON.stringify({
                devReload: { requestId: 1, extensionId },
              }),
            );
            return;
          }
          const reply = frame.devResult;
          if (!reply || reply.requestId !== 1)
            throw new Error("Uncorrelated developer result");
          if (reply.error) throw new Error(reply.error);
          if (!reply.result)
            throw new Error("Developer reload omitted its result");
          clearTimeout(timeout);
          resolve(reply.result);
        } catch (error) {
          fail(error);
        }
      });
    });
  } finally {
    socket.close();
    await waitFor(
      "Developer socket release",
      () => socket.readyState === WebSocket.CLOSED,
      { timeoutMs: 4000 },
    );
    credential.token = "";
  }
}
