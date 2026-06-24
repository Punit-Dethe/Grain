import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { Dropdown } from "../ui/Dropdown";
import { SettingContainer } from "../ui/SettingContainer";

interface RollingWindowProps {
  descriptionMode?: "tooltip" | "inline";
  grouped?: boolean;
}

// [GRAIN] DUMMY placeholder — UI only, not yet wired to the backend. Once the
// live rolling-window transcription path lands, this will drive the buffer
// duration. For now it holds local state so the control is interactive.
const OPTIONS = ["sec15", "sec20", "sec25", "sec30", "sec45", "sec60"] as const;

export const RollingWindow: React.FC<RollingWindowProps> = ({
  descriptionMode = "inline",
  grouped = false,
}) => {
  const { t } = useTranslation();
  const [value, setValue] = useState<string>("sec20");

  const options = OPTIONS.map((v) => ({
    value: v,
    label: t(`settings.speechToText.rollingWindow.options.${v}`),
  }));

  return (
    <SettingContainer
      title={t("settings.speechToText.rollingWindow.label")}
      description={t("settings.speechToText.rollingWindow.description")}
      descriptionMode={descriptionMode}
      grouped={grouped}
    >
      <Dropdown
        options={options}
        selectedValue={value}
        onSelect={(v) => setValue(v)}
        disabled={false}
      />
    </SettingContainer>
  );
};
