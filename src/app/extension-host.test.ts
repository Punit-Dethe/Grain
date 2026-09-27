import { setImmediate } from "node:timers/promises";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const api = vi.hoisted(() => ({ listen: vi.fn(), emit: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => api);

type Handler = (event: { payload: unknown }) => void;
const handlers = new Map<string, Handler>();
const unlisteners: ReturnType<typeof vi.fn>[] = [];
const made: TestWorker[] = [];

class TestWorker {
  static fail = false;
  onerror: ((event: { message: string }) => void) | null = null;
  onmessage: ((event: { data: unknown }) => void) | null = null;
  terminate = vi.fn();
  constructor(public url: string) {
    if (TestWorker.fail) throw new Error("cannot construct");
    made.push(this);
  }
}

function register(name: string, handler: Handler) {
  handlers.set(name, handler);
  const unlisten = vi.fn(() => handlers.delete(name));
  unlisteners.push(unlisten);
  return unlisten;
}

let page: EventTarget & { __GRAIN_SUPERVISOR_GENERATION__: number };
let nextUrl: number;

beforeEach(() => {
  vi.resetModules();
  vi.clearAllMocks();
  handlers.clear();
  unlisteners.length = 0;
  made.length = 0;
  TestWorker.fail = false;
  nextUrl = 0;
  page = Object.assign(new EventTarget(), {
    __GRAIN_SUPERVISOR_GENERATION__: 7,
  });
  vi.stubGlobal("window", page);
  vi.stubGlobal("Worker", TestWorker);
  vi.spyOn(URL, "createObjectURL").mockImplementation(
    () => `blob:${++nextUrl}`,
  );
  vi.spyOn(URL, "revokeObjectURL").mockImplementation(() => undefined);
  api.listen.mockImplementation(async (name: string, handler: Handler) =>
    register(name, handler),
  );
  api.emit.mockResolvedValue(undefined);
});

afterEach(() => {
  page.dispatchEvent(new Event("pagehide"));
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

async function start() {
  // Executes the actual non-rendering supervisor; only its event/Worker boundary
  // is substituted. No alternate app or visual review path is created.
  await import("./extension-host");
  await setImmediate();
}

function spawn(token: string, id = "tools") {
  handlers.get("ext-host://spawn")?.({
    payload: {
      ext_id: id,
      token,
      entry_source: "grain.actions({ping: () => 1})",
    },
  });
}

function kill(token: string) {
  handlers.get("ext-host://kill")?.({ payload: { ext_id: "tools", token } });
}

describe("production supervisor ownership", () => {
  it("announces readiness only after both listeners, with its generation", async () => {
    await start();
    expect(api.listen.mock.calls.map(([name]) => name)).toEqual([
      "ext-host://spawn",
      "ext-host://kill",
    ]);
    expect(api.emit).toHaveBeenCalledExactlyOnceWith("ext-host://ready", {
      generation: 7,
    });
  });

  it("ignores duplicate spawn, stale kill and stale callbacks after replacement", async () => {
    await start();
    spawn("old");
    const oldError = made[0].onerror!;
    const oldFatal = made[0].onmessage!;
    spawn("old");
    expect(made).toHaveLength(1);
    spawn("new");
    expect(made[0].terminate).toHaveBeenCalledOnce();
    kill("old");
    oldError({ message: "late error" });
    oldFatal({ data: { type: "fatal", reason: "late fatal" } });
    expect(made[1].terminate).not.toHaveBeenCalled();
    expect(api.emit).not.toHaveBeenCalledWith(
      "ext-host://died",
      expect.anything(),
    );
    made[1].onmessage!({ data: { type: "fatal", reason: "current failure" } });
    expect(api.emit).toHaveBeenCalledWith(
      "ext-host://died",
      expect.objectContaining({
        token: "new",
        reason: "current failure",
      }),
    );
    expect(made[1].terminate).toHaveBeenCalledOnce();
    expect(URL.revokeObjectURL).toHaveBeenCalledTimes(2);
  });

  it("frees listeners and every worker URL exactly once on page close", async () => {
    await start();
    spawn("one");
    spawn("two", "other");
    const retainedSpawn = handlers.get("ext-host://spawn")!;
    page.dispatchEvent(new Event("pagehide"));
    page.dispatchEvent(new Event("pagehide"));
    retainedSpawn({
      payload: { ext_id: "late", token: "late", entry_source: "" },
    });
    expect(made).toHaveLength(2);
    for (const worker of made) expect(worker.terminate).toHaveBeenCalledOnce();
    for (const unlisten of unlisteners) expect(unlisten).toHaveBeenCalledOnce();
    expect(URL.revokeObjectURL).toHaveBeenCalledTimes(2);
  });

  it("cleans a listener that registers after page close without signaling ready", async () => {
    let finish!: (unlisten: () => void) => void;
    api.listen.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    );
    await start();
    page.dispatchEvent(new Event("pagehide"));
    const unlisten = vi.fn();
    finish(unlisten);
    await setImmediate();
    expect(unlisten).toHaveBeenCalledOnce();
    expect(api.listen).toHaveBeenCalledTimes(1);
    expect(api.emit).not.toHaveBeenCalled();
  });

  it("cleans partially initialized listeners and reports startup failure", async () => {
    api.listen.mockImplementationOnce(async (name, handler) =>
      register(name, handler),
    );
    api.listen.mockRejectedValueOnce(new Error("listener failed"));
    await start();
    expect(unlisteners[0]).toHaveBeenCalledOnce();
    expect(api.emit).toHaveBeenCalledExactlyOnceWith("ext-host://failed", {
      generation: 7,
      reason: "Error: listener failed",
    });
  });

  it("cleans failed worker construction and twenty sequential replacements", async () => {
    await start();
    TestWorker.fail = true;
    spawn("failure");
    expect(URL.revokeObjectURL).toHaveBeenCalledWith("blob:1");
    expect(api.emit).toHaveBeenCalledWith(
      "ext-host://died",
      expect.objectContaining({ token: "failure" }),
    );
    TestWorker.fail = false;
    for (let i = 0; i < 20; i++) spawn(`token-${i}`);
    kill("token-19");
    expect(made).toHaveLength(20);
    for (const worker of made) expect(worker.terminate).toHaveBeenCalledOnce();
    expect(URL.revokeObjectURL).toHaveBeenCalledTimes(21);
  });
});
