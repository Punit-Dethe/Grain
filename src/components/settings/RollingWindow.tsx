import React from "react";
import { useTranslation } from "react-i18next";
import { useSettings } from "../../hooks/useSettings";
import { Dropdown } from "../ui/Dropdown";
import { SettingContainer } from "../ui/SettingContainer";

interface RollingWindowProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

// [GRAIN] Rolling-window hard-cut length (seconds) for the real-time transcribe
// path. Wired to `rolling_window_seconds` in AppSettings; the backend clamps to
// [15, 60] and `RollingSession::start` reads it per session. The selected value
// comes straight from settings (default 15) — no hardcoded UI default. Persisted
// through `updateSetting`, which routes to `changeRollingWindowSecondsSetting`
// via the store's `settingUpdaters` map and gives optimistic update + rollback.
const OPTION_SECONDS = [15, 20, 25, 30, 45, 60] as const;

export const RollingWindow: React.FC<RollingWindowProps> = ({
  descriptionMode = "inline",
  grouped = false,
}) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting } = useSettings();

  // Reflect the backend value. Fall back to the lowest option only until the
  // settings store has loaded; the real default (15) comes from the backend.
  const current = getSetting("rolling_window_seconds") ?? OPTION_SECONDS[0];

  const options = OPTION_SECONDS.map((s) => ({
    value: String(s),
    label: t(`settings.speechToText.rollingWindow.options.sec${s}`),
  }));

  const handleSelect = (value: string) => {
    const seconds = Number(value);
    if (!Number.isFinite(seconds)) return;
    // `updateSetting` optimistically updates the store, calls the backend via
    // `settingUpdaters`, and rolls back if the command fails.
    void updateSetting("rolling_window_seconds", seconds);
  };

  return (
    <SettingContainer
      title={t("settings.speechToText.rollingWindow.label")}
      description={t("settings.speechToText.rollingWindow.description")}
      descriptionMode={descriptionMode}
      grouped={grouped}
    >
      <Dropdown
        options={options}
        selectedValue={String(current)}
        onSelect={handleSelect}
        disabled={false}
      />
    </SettingContainer>
  );
};
