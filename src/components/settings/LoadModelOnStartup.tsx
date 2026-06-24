import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { ToggleSwitch } from "../ui/ToggleSwitch";

interface LoadModelOnStartupProps {
  descriptionMode?: "tooltip" | "inline";
  grouped?: boolean;
}

// [GRAIN] DUMMY placeholder — UI only, not yet wired to the backend. Will preload
// the local transcription model at app start once implemented; for now it holds
// local state so the toggle is interactive.
export const LoadModelOnStartup: React.FC<LoadModelOnStartupProps> = ({
  descriptionMode = "inline",
  grouped = false,
}) => {
  const { t } = useTranslation();
  const [on, setOn] = useState(false);

  return (
    <ToggleSwitch
      checked={on}
      onChange={setOn}
      label={t("settings.speechToText.loadOnStartup.label")}
      description={t("settings.speechToText.loadOnStartup.description")}
      descriptionMode={descriptionMode}
      grouped={grouped}
    />
  );
};
