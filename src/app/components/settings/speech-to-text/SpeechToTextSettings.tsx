import React from "react";
import { useTranslation } from "react-i18next";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { ModelPicker } from "./ModelPicker";
import { ModelUnloadTimeoutSetting } from "../ModelUnloadTimeout";
import { ExtensionAnchor } from "../experimentations/ExtensionSettings";

export const SpeechToTextSettings: React.FC = () => {
  const { t } = useTranslation();
  return (
    <div className="max-w-4xl w-full mx-auto space-y-7">
      <div className="px-1">
        <h1 className="text-xl font-semibold mb-1">
          {t("settings.speechToText.title")}
        </h1>
        <p className="text-sm text-ink-soft">
          {t("settings.speechToText.description")}
        </p>
      </div>

      {/* 1) Both model roles in one picker: two slots that say what each is
          for and whether it is filled, over one tabbed library. */}
      <ModelPicker />

      {/* 2) On-device model behaviour. Flow geometry is fixed by the reviewed
          Parakeet TDT v2/v3 contract and is intentionally not configurable. */}
      <SettingsGroup title={t("settings.speechToText.groups.engine")}>
        <ModelUnloadTimeoutSetting descriptionMode="tooltip" grouped />
      </SettingsGroup>

      {/* [GRAIN] SPEC §4.3 anchor — model-related extensions render beside the
          models they extend. Nothing renders until one anchors here. */}
      <ExtensionAnchor anchor="models.after" />
    </div>
  );
};
