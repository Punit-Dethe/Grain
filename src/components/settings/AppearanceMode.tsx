/**
 * [GRAIN] Appearance row — the three modes the backend actually stores.
 *
 * `AppearanceToggle`, which this replaces in UI 2.0, is a two-state switch
 * built on the deprecated `isSettingsDark` / `toggleSettings` aliases, and its
 * copy still promises an independently themed Quick Panel that no longer
 * exists. `AppSettings.theme` is `system | light | dark`; `system` has no
 * representation in a checkbox, so a user who wants Grain to follow the OS
 * cannot say so. This row is the only place that choice is expressible.
 */
import React from "react";
import { useTranslation } from "react-i18next";
import { SettingContainer } from "../ui/SettingContainer";
import type { ThemeMode } from "@/bindings";
import { useTheme } from "../../contexts/ThemeContext";

const MODES: readonly ThemeMode[] = ["system", "light", "dark"] as const;

export const AppearanceMode: React.FC = () => {
  const { t } = useTranslation();
  const { mode, setMode } = useTheme();

  return (
    <SettingContainer
      title={t("ui2.appearance.title")}
      description={t("ui2.appearance.description")}
      descriptionMode="tooltip"
      grouped
    >
      <div
        className="view-switch"
        role="radiogroup"
        aria-label={t("ui2.appearance.title")}
      >
        {MODES.map((option) => (
          <button
            key={option}
            type="button"
            role="radio"
            aria-checked={mode === option}
            className={mode === option ? "active" : ""}
            onClick={() => setMode(option)}
          >
            {t(`ui2.appearance.modes.${option}`)}
          </button>
        ))}
      </div>
    </SettingContainer>
  );
};
