import { afterEach, describe, expect, it, vi } from "vitest";
import type { TranscriptionJob } from "@/bindings";

const api = vi.hoisted(() => ({ listen: vi.fn(), snapshot: vi.fn() }));
vi.mock("@/bindings", () => ({
  commands: { getTranscriptionJobs: api.snapshot },
  events: { transcriptionJob: { listen: api.listen } },
}));
import { areModelsLocked, watchModelJobs } from "./modelJobs";

const queued = { entry_id: 1, status: { state: "queued" } } as TranscriptionJob;
const cancelled = {
  entry_id: 1,
  status: { state: "cancelled" },
} as TranscriptionJob;
let close: (() => void) | undefined;
afterEach(() => {
  close?.();
  close = undefined;
  vi.clearAllMocks();
});

describe("catalogue file-job subscription", () => {
  it("lets an event during the initial snapshot override stale snapshot data", async () => {
    let deliver!: (event: { payload: TranscriptionJob }) => void;
    let resolve!: (jobs: TranscriptionJob[]) => void;
    const stop = vi.fn();
    api.listen.mockImplementation(async (callback) => {
      deliver = callback;
      return stop;
    });
    api.snapshot.mockImplementation(
      () =>
        new Promise<TranscriptionJob[]>((done) => {
          resolve = done;
        }),
    );
    close = watchModelJobs();
    expect(areModelsLocked()).toBe(true);
    await vi.waitFor(() => expect(resolve).toBeDefined());
    deliver({ payload: cancelled });
    resolve([queued]);
    await vi.waitFor(() => expect(areModelsLocked()).toBe(false));
    deliver({ payload: queued });
    expect(areModelsLocked()).toBe(true);
    close();
    close = undefined;
    expect(stop).toHaveBeenCalledOnce();
    deliver({ payload: cancelled });
    expect(areModelsLocked()).toBe(true);
  });

  it("releases a late listener when the catalogue has already closed", async () => {
    let resolve!: (stop: () => void) => void;
    const stop = vi.fn();
    api.listen.mockImplementation(
      () =>
        new Promise<() => void>((done) => {
          resolve = done;
        }),
    );
    close = watchModelJobs();
    close();
    close = undefined;
    resolve(stop);
    await vi.waitFor(() => expect(stop).toHaveBeenCalledOnce());
    expect(api.snapshot).not.toHaveBeenCalled();
  });

  it("keeps observing jobs after a failed snapshot without locking the page forever", async () => {
    let deliver!: (event: { payload: TranscriptionJob }) => void;
    api.listen.mockImplementation(async (callback) => {
      deliver = callback;
      return vi.fn();
    });
    api.snapshot.mockRejectedValueOnce(new Error("Snapshot unavailable"));
    const log = vi.spyOn(console, "error").mockImplementation(() => {});
    try {
      close = watchModelJobs();
      await vi.waitFor(() => expect(areModelsLocked()).toBe(false));
      deliver({ payload: queued });
      expect(areModelsLocked()).toBe(true);
      deliver({ payload: cancelled });
      expect(areModelsLocked()).toBe(false);
    } finally {
      log.mockRestore();
    }
  });
});
