/* eslint-disable i18next/no-literal-string */
/**
 * [GRAIN] AppearanceToggle — Light / Dark mode row for the Advanced settings tab.
 * Reads and writes through the shared ThemeContext so changing this row
 * instantly updates the Quick Panel toggle and vice-versa.
 */
import React from "react";
import { useTheme } from "../../contexts/ThemeContext";
import { SettingContainer } from "../ui/SettingContainer";

export const AppearanceToggle: React.FC = () => {
  const { isSettingsDark, toggleSettings } = useTheme();

  return (
    <SettingContainer
      title="Appearance"
      description="Switch the Settings window between light and dark mode. The Quick Panel appearance can be toggled independently."
      descriptionMode="tooltip"
      grouped={true}
    >
      <label className="inline-flex items-center cursor-pointer transition-transform duration-100 active:scale-90">
        <input
          type="checkbox"
          className="sr-only peer"
          checked={isSettingsDark}
          onChange={toggleSettings}
        />
        {/* Same hardware-toggle style as ToggleSwitch */}
        <div
          className="relative w-8 h-[18px] rounded-full transition-colors duration-200 peer-focus:outline-none peer-focus:ring-2 peer-focus:ring-[var(--focus)] after:content-[''] after:absolute after:top-[2px] after:start-[2px] after:h-[14px] after:w-[14px] after:rounded-full after:shadow-[0_1px_3px_rgba(0,0,0,0.35)] after:transition-all after:duration-200 peer-checked:after:translate-x-[14px] peer-checked:after:bg-[var(--color-accent)] after:bg-[#f0ebe3] rtl:peer-checked:after:-translate-x-[14px]"
          style={{ backgroundColor: "#0a0a0a" }}
        />
      </label>
    </SettingContainer>
  );
};
