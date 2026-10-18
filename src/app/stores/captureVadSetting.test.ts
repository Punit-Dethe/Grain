import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { commands, type AppSettings } from "@/bindings";
import { useSettingsStore } from "./settingsStore";

vi.mock("@/bindings", () => ({
  commands: { changeVadEnabledSetting: vi.fn() },
}));

beforeEach(() => {
  vi.clearAllMocks();
  useSettingsStore.setState({
    settings: { vad_enabled: true, long_form_chunking: true } as AppSettings,
    isUpdating: {},
  });
});

afterEach(() => vi.restoreAllMocks());

describe("capture VAD setting", () => {
  it("persists the switch through IPC without changing long-form chunking", async () => {
    vi.mocked(commands.changeVadEnabledSetting).mockResolvedValue({
      status: "ok",
      data: null,
    });
    await useSettingsStore.getState().updateSetting("vad_enabled", false);
    expect(commands.changeVadEnabledSetting).toHaveBeenCalledExactlyOnceWith(
      false,
    );
    expect(useSettingsStore.getState().settings).toMatchObject({
      vad_enabled: false,
      long_form_chunking: true,
    });
    expect(useSettingsStore.getState().isUpdating.vad_enabled).toBe(false);
  });

  it("restores the previous switch state when persistence fails", async () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    vi.mocked(commands.changeVadEnabledSetting).mockResolvedValue({
      status: "error",
      error: "Settings could not be saved",
    });
    await useSettingsStore.getState().updateSetting("vad_enabled", false);
    expect(useSettingsStore.getState().settings?.vad_enabled).toBe(true);
    expect(useSettingsStore.getState().isUpdating.vad_enabled).toBe(false);
  });
});
