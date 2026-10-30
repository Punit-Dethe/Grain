import { runInNewContext } from "node:vm";
import { setImmediate } from "node:timers/promises";
import { describe, expect, it, vi } from "vitest";
import { GRAIN_RUNTIME_JS } from "./extension-runtime";

type ToolHandler = (...args: unknown[]) => unknown;
interface ToolApi {
  actions(map: Record<string, ToolHandler>): void;
  storage: { get(key: string): Promise<unknown> };
  net: { fetch(url: string): Promise<unknown> };
}

// Executes the production worker wire shim. No app rendering or alternate UI path.
function worker() {
  class Socket {
    static instance: Socket;
    sent: string[] = [];
    onopen?: () => void;
    onclose?: () => void;
    onmessage?: (event: { data: string }) => void;
    constructor() {
      Socket.instance = this;
    }
    send(frame: string) {
      this.sent.push(frame);
    }
  }
  const self = {
    postMessage: vi.fn(),
    grain: undefined as ToolApi | undefined,
  };
  runInNewContext(GRAIN_RUNTIME_JS, {
    __GRAIN_EXT_ID__: "com.example.tools",
    __GRAIN_TOKEN__: "test-token",
    __GRAIN_CAPS__: ["storage", "net:api.example.com"],
    __GRAIN_ACTIVATION__: { TranscriptionComplete: { text: "private" } },
    WebSocket: Socket,
    self,
  });
  const socket = Socket.instance;
  socket.onopen?.();
  async function call(method: string, params: unknown, call_id = 1) {
    socket.onmessage?.({
      data: JSON.stringify({ call: { method, params, call_id } }),
    });
    await setImmediate();
    return JSON.parse(socket.sent[socket.sent.length - 1]) as {
      callres: { ok?: unknown; err?: string };
    };
  }
  return { grain: self.grain!, socket, call };
}

describe("tool-only production worker", () => {
  it("preserves supported tagged results even when their payload is null or empty", async () => {
    const { grain, call } = worker();
    for (const result of [
      { ok: null },
      { ok: "" },
      { ok: { body: "done", details: [{ label: "id", value: "123" }] } },
      { error: { class: "network", message: "private diagnostic" } },
    ]) {
      grain.actions({ read: () => result });
      expect((await call("action", { action: "read" })).callres.ok).toEqual(
        result,
      );
    }
  });

  it("forwards only the write key alongside validated arguments", async () => {
    const { grain, call } = worker();
    const handler = vi.fn(() => ({ ok: { body: "done" } }));
    grain.actions({ write: handler });
    await call("action", {
      action: "write",
      arguments: { item: "123" },
      idempotencyKey: "owned-write-key",
      screen: "private",
      transcript: "private",
      otherExtensions: ["private"],
    });
    expect(handler).toHaveBeenCalledExactlyOnceWith(
      { item: "123" },
      { idempotencyKey: "owned-write-key" },
    );
  });

  it("preserves reserved envelopes for host classification instead of wrapping them as successful data", async () => {
    const { grain, call } = worker();
    for (const result of [
      { ok: null, error: null },
      { ok: false },
      { error: null },
      { needsInteraction: null },
    ]) {
      grain.actions({ read: () => result });
      expect((await call("action", { action: "read" })).callres.ok).toEqual(
        result,
      );
    }
  });

  it("has no retired APIs or ambient activation and sends exact tool arguments only", async () => {
    const { grain, call, socket } = worker();
    expect(Object.keys(grain).sort()).toEqual([
      "actions",
      "auth",
      "caps",
      "extId",
      "log",
      "net",
      "storage",
    ]);
    const handler = vi.fn(() => ({ body: "done" }));
    grain.actions({ read: handler });
    socket.onmessage?.({
      data: JSON.stringify({ TranscriptionComplete: { text: "private" } }),
    });
    expect(
      (await call("request", { request: "private" })).callres.err,
    ).toContain("no handler");
    expect(
      (await call("sessionStage", { text: "private" })).callres.err,
    ).toContain("no handler");
    expect(handler).not.toHaveBeenCalled();
    expect(
      (
        await call("action", {
          action: "read",
          arguments: { query: "explicit" },
          screen: "private",
        })
      ).callres.ok,
    ).toEqual({ ok: { body: "done" } });
    expect(handler).toHaveBeenCalledExactlyOnceWith(
      { query: "explicit" },
      { idempotencyKey: null },
    );
  });

  it("cannot call inherited names or replace registered functions by mutating the map", async () => {
    const { grain, call } = worker();
    const original = vi.fn(() => ({ body: "original" }));
    const injected = vi.fn(() => ({ body: "injected" }));
    const map = Object.assign(Object.create({ inherited: injected }), {
      read: original,
    });
    grain.actions(map);
    map.read = injected;
    for (const action of [
      "inherited",
      "constructor",
      "toString",
      "__proto__",
    ]) {
      expect((await call("action", { action })).callres.ok).toMatchObject({
        error: { class: "not_found" },
      });
    }
    await call("action", { action: "read", arguments: {} });
    expect(original).toHaveBeenCalledOnce();
    expect(injected).not.toHaveBeenCalled();
  });

  it("authenticates first, proxies allowed requests, and releases waiters on socket close", async () => {
    const { grain, socket } = worker();
    expect(JSON.parse(socket.sent[0])).toMatchObject({
      token: "test-token",
      client: "com.example.tools",
    });
    const result = grain.storage.get("key");
    const frame = JSON.parse(socket.sent[1]);
    expect(frame.req).toMatchObject({
      method: "storage.get",
      params: { key: "key" },
    });
    socket.onmessage?.({
      data: JSON.stringify({ res: { id: frame.req.id, ok: "value" } }),
    });
    await expect(result).resolves.toBe("value");
    const pending = grain.net.fetch("https://api.example.com/read");
    const rejected = expect(pending).rejects.toThrow("tool connection closed");
    socket.onclose?.();
    await rejected;
    const count = socket.sent.length;
    await expect(grain.storage.get("after-close")).rejects.toThrow(
      "tool connection closed",
    );
    expect(socket.sent).toHaveLength(count);
  });
});
