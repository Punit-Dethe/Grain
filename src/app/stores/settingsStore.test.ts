import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import type { AppSettings } from "@/bindings";
import { useSettingsStore } from "./settingsStore";

vi.mock("@tauri-apps/api/core", async (original) => ({
  ...(await original<typeof import("@tauri-apps/api/core")>()),
  invoke: vi.fn(),
}));

beforeEach(() => {
  vi.clearAllMocks();
  useSettingsStore.setState({
    settings: { agent_enabled: false, snippets_enabled: false } as AppSettings,
    isUpdating: {},
  });
});

describe("core feature switches", () => {
  it.each([
    ["agent_enabled", "change_agent_enabled_setting"],
    ["snippets_enabled", "change_snippets_enabled_setting"],
  ] as const)(
    "persists %s independently of the extension registry",
    async (key, command) => {
      const persisted = {
        agent_enabled: false,
        snippets_enabled: false,
      } as AppSettings;
      vi.mocked(invoke).mockImplementation(async (name, args) => {
        if (name === "get_app_settings") return { ...persisted };
        if (name !== command) throw "Unexpected command";
        persisted[key] = (args as { enabled: boolean }).enabled;
        return null;
      });

      for (const enabled of [true, false]) {
        await useSettingsStore.getState().updateSetting(key, enabled);
        await useSettingsStore.getState().refreshSettings();
        expect(useSettingsStore.getState().getSetting(key)).toBe(enabled);
        expect(invoke).toHaveBeenCalledWith(command, { enabled });
      }
      expect(
        vi
          .mocked(invoke)
          .mock.calls.some(([name]) => name === "extension_set_enabled"),
      ).toBe(false);
    },
  );

  it.each([
    ["agent_enabled", "change_agent_enabled_setting"],
    ["snippets_enabled", "change_snippets_enabled_setting"],
  ] as const)(
    "rolls back %s when the backend rejects the change",
    async (key, command) => {
      // Tauri rejects with a string; the real generated binding turns this into
      // a structured error instead of rejecting its Promise.
      vi.mocked(invoke).mockRejectedValue("Cannot change shortcut");
      const logged = vi.spyOn(console, "error").mockImplementation(() => {});
      try {
        for (const original of [false, true]) {
          useSettingsStore.getState().setSettings({
            ...useSettingsStore.getState().settings!,
            [key]: original,
          });
          await useSettingsStore.getState().updateSetting(key, !original);
          expect(invoke).toHaveBeenLastCalledWith(command, {
            enabled: !original,
          });
          expect(useSettingsStore.getState().getSetting(key)).toBe(original);
          expect(useSettingsStore.getState().isUpdatingKey(key)).toBe(false);
        }
        expect(logged).toHaveBeenCalled();
      } finally {
        logged.mockRestore();
      }
    },
  );
});
