import { beforeEach, describe, expect, it, vi } from "vitest";
import { commands, type PpPoolView } from "@/bindings";
import { initPpPool, usePpPoolStore } from "./ppPoolStore";

vi.mock("@/bindings", () => ({
  commands: {
    ppGetPool: vi.fn(),
    ppReorderProviders: vi.fn(),
    ppSetFallbackEnabled: vi.fn(),
  },
}));

const ppView: PpPoolView = {
  fallback_enabled: false,
  providers: [],
  selected_provider_id: "",
  configured_provider_ids: [],
  providers_with_keys: [],
  models: {},
};

beforeEach(() => {
  vi.clearAllMocks();
  usePpPoolStore.setState({
    view: null,
    loading: true,
    error: null,
    fallbackEnabled: false,
    providers: [],
    selectedProviderId: "",
    providersWithKeys: new Set(),
    configuredProviderIds: new Set(),
    models: {},
  });
});

describe("provider pool initialization", () => {
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

describe("provider priority persistence", () => {
  it("refreshes canonical settings after a successful reorder", async () => {
    vi.mocked(commands.ppReorderProviders).mockResolvedValue({
      status: "ok",
      data: null,
    });
    vi.mocked(commands.ppGetPool).mockResolvedValue({
      status: "ok",
      data: {
        ...ppView,
        fallback_enabled: true,
        configured_provider_ids: ["custom"],
      },
    });
    await usePpPoolStore.getState().reorderProviders(["custom", "openai"]);
    expect(commands.ppReorderProviders).toHaveBeenCalledWith([
      "custom",
      "openai",
    ]);
    expect(usePpPoolStore.getState().fallbackEnabled).toBe(true);
    expect(usePpPoolStore.getState().configuredProviderIds.has("custom")).toBe(
      true,
    );
  });

  it("reports rejected writes and failed refreshes without optimistic order", async () => {
    vi.mocked(commands.ppReorderProviders).mockResolvedValueOnce({
      status: "error",
      error: "Stale list",
    });
    await expect(
      usePpPoolStore.getState().reorderProviders(["unknown"]),
    ).rejects.toThrow("Stale list");
    expect(commands.ppGetPool).not.toHaveBeenCalled();
    expect(usePpPoolStore.getState().error).toBe("Stale list");
    vi.mocked(commands.ppReorderProviders).mockResolvedValueOnce({
      status: "ok",
      data: null,
    });
    vi.mocked(commands.ppGetPool).mockRejectedValueOnce(
      new Error("IPC unavailable"),
    );
    await expect(
      usePpPoolStore.getState().reorderProviders([]),
    ).rejects.toThrow("IPC unavailable");
    expect(usePpPoolStore.getState().error).toBe("IPC unavailable");
  });
});
