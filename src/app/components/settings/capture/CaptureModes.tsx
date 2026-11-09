import React from "react";
import { useTranslation } from "react-i18next";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { ShortcutInput } from "../ShortcutInput";
import { ModelUnloadTimeoutSetting } from "../ModelUnloadTimeout";

export const CaptureModes: React.FC = () => {
  const { t } = useTranslation();
  return (
    <SettingsGroup title={t("ui2.capture.group")}>
      <ShortcutInput shortcutId="transcribe" grouped />
      <ModelUnloadTimeoutSetting descriptionMode="tooltip" grouped />
    </SettingsGroup>
  );
};
