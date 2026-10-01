import { createServer } from "node:http";
import { randomUUID } from "node:crypto";

export const FIXTURE_ID = "com.grain.harness.lifecycle";
const MAX_BODY = 1024 * 1024;

export function nextReply(body) {
  if (
    body.model !== "harness-scripted" ||
    !Array.isArray(body.messages) ||
    !Array.isArray(body.tools)
  )
    throw new Error("Unexpected model request shape");
  const instruction = body.messages.find(
    (message) =>
      message.role === "user" &&
      typeof message.content === "string" &&
      message.content.startsWith("Harness request:"),
  )?.content;
  if (!instruction) throw new Error("Missing harness instruction");
  if (instruction.endsWith("model_wait"))
    return { content: "Harness delayed model reply", delayMs: 15000 };
  const target = instruction.endsWith("slow_hello") ? "slow_hello" : "hello";
  const results = body.messages.filter((message) => message.role === "tool");
  const offered = body.tools.map((tool) => tool.function);
  const call = (name, args) => {
    if (!offered.some((tool) => tool.name === name))
      throw new Error(`Tool was not offered: ${name}`);
    return {
      tool_calls: [
        {
          id: `harness_${randomUUID()}`,
          type: "function",
          function: { name, arguments: JSON.stringify(args) },
        },
      ],
    };
  };
  if (results.length === 0) {
    if (offered.some((tool) => tool.name.startsWith("act__")))
      throw new Error("Initial Agent frame exposed action schemas");
    return call("search_tools", { extension_id: FIXTURE_ID, query: target });
  }
  if (results.length === 1)
    return call("load_extension", {
      extension_id: FIXTURE_ID,
      tool_ids: [target],
    });
  if (results.length === 2) {
    const actionTools = offered.filter((tool) => tool.name.startsWith("act__"));
    if (actionTools.length !== 1)
      throw new Error("Selected loading did not expose exactly one action");
    const expectedTitle =
      target === "hello" ? "Harness fast hello" : "Harness slow hello";
    if (!actionTools[0].description.includes(expectedTitle))
      throw new Error("Loaded the wrong native action schema");
    return call(actionTools[0].name, {});
  }
  return { content: `Harness observed result: ${results.at(-1).content}` };
}

export async function startModel() {
  const journal = [];
  const sockets = new Set();
  const timers = new Set();
  const server = createServer(async (request, response) => {
    let size = 0;
    const chunks = [];
    try {
      if (request.method !== "POST" || request.url !== "/v1/chat/completions")
        throw new Error("Unsupported fixture endpoint");
      if (request.headers.authorization)
        throw new Error("Credentials must not reach the scripted provider");
      for await (const chunk of request) {
        size += chunk.length;
        if (size > MAX_BODY)
          throw new Error("Fixture request exceeded its limit");
        chunks.push(chunk);
      }
      const body = JSON.parse(Buffer.concat(chunks).toString("utf8"));
      const reply = nextReply(body);
      const entry = {
        sequence: journal.length + 1,
        offered: body.tools.map((tool) => tool.function.name),
        returned: reply.tool_calls?.map((tool) => tool.function.name) ?? [],
        state: "received",
      };
      journal.push(entry);
      if (journal.length > 4096) throw new Error("Model journal overflow");
      if (reply.delayMs) {
        await new Promise((resolve) => {
          const timer = setTimeout(done, reply.delayMs);
          timers.add(timer);
          function done() {
            clearTimeout(timer);
            timers.delete(timer);
            response.off("close", done);
            resolve();
          }
          response.once("close", done);
        });
      }
      if (response.destroyed) {
        entry.state = "cancelled";
        return;
      }
      response.writeHead(200, { "Content-Type": "application/json" });
      response.end(
        JSON.stringify({
          id: `harness_${entry.sequence}`,
          object: "chat.completion",
          created: 0,
          model: "harness-scripted",
          choices: [
            {
              index: 0,
              message: {
                role: "assistant",
                content: reply.content ?? null,
                ...(reply.tool_calls ? { tool_calls: reply.tool_calls } : {}),
              },
              finish_reason: reply.tool_calls ? "tool_calls" : "stop",
            },
          ],
          usage: { prompt_tokens: 1, completion_tokens: 1, total_tokens: 2 },
        }),
      );
      entry.state = "replied";
    } catch (error) {
      journal.push({
        sequence: journal.length + 1,
        state: "error",
        error: String(error.message).slice(0, 500),
      });
      if (!response.destroyed) {
        response.writeHead(400, { "Content-Type": "application/json" });
        response.end(JSON.stringify({ error: { message: error.message } }));
      }
    }
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
    async close() {
      for (const timer of timers) clearTimeout(timer);
      for (const socket of sockets) socket.destroy();
      await new Promise((resolve) => server.close(resolve));
    },
  };
}
