import { beforeEach, describe, expect, it, vi } from "vitest";
import { commands, type PpPoolView, type SttPoolView } from "@/bindings";
import { initPpPool, usePpPoolStore } from "./ppPoolStore";
import { initSttPool, useSttPoolStore } from "./sttPoolStore";

vi.mock("@/bindings", () => ({
  commands: {
    ppGetPool: vi.fn(),
    sttGetPool: vi.fn(),
  },
}));

const sttView: SttPoolView = {
  smart_rotation: false,
  providers: [],
  providers_with_keys: [],
};

const ppView: PpPoolView = {
  smart_rotation: false,
  providers: [],
  selected_provider_id: "",
  providers_with_keys: [],
  models: {},
};

beforeEach(() => {
  vi.clearAllMocks();
  useSttPoolStore.setState({
    view: null,
    loading: true,
    error: null,
    smartRotation: false,
    providers: [],
    cloudProviders: [],
    localProvider: undefined,
    providersWithKeys: new Set(),
  });
  usePpPoolStore.setState({
    view: null,
    loading: true,
    error: null,
    smartRotation: false,
    providers: [],
    selectedProviderId: "",
    providersWithKeys: new Set(),
    models: {},
  });
});

describe("provider pool initialization", () => {
  it("coalesces concurrent STT initialization and skips after success", async () => {
    vi.mocked(commands.sttGetPool).mockResolvedValue({
      status: "ok",
      data: sttView,
    });

    const first = initSttPool();
    const second = initSttPool();

    expect(second).toBe(first);
    await Promise.all([first, second]);
    await initSttPool();
    expect(commands.sttGetPool).toHaveBeenCalledTimes(1);
  });

  it("allows STT initialization to retry after failure", async () => {
    vi.mocked(commands.sttGetPool)
      .mockResolvedValueOnce({ status: "error", error: "offline" })
      .mockResolvedValueOnce({ status: "ok", data: sttView });

    await expect(initSttPool()).rejects.toThrow("offline");
    await expect(initSttPool()).resolves.toBeUndefined();
    expect(commands.sttGetPool).toHaveBeenCalledTimes(2);
  });

  it("coalesces concurrent post-processing initialization", async () => {
    vi.mocked(commands.ppGetPool).mockResolvedValue({
      status: "ok",
      data: ppView,
    });

    const first = initPpPool();
    const second = initPpPool();

    expect(second).toBe(first);
    await Promise.all([first, second]);
    await initPpPool();
    expect(commands.ppGetPool).toHaveBeenCalledTimes(1);
  });

  it("allows post-processing initialization to retry after failure", async () => {
    vi.mocked(commands.ppGetPool)
      .mockRejectedValueOnce(new Error("IPC unavailable"))
      .mockResolvedValueOnce({ status: "ok", data: ppView });

    await expect(initPpPool()).rejects.toThrow("IPC unavailable");
    expect(usePpPoolStore.getState()).toMatchObject({
      loading: false,
      error: "IPC unavailable",
    });
    await expect(initPpPool()).resolves.toBeUndefined();
    expect(commands.ppGetPool).toHaveBeenCalledTimes(2);
  });
});
