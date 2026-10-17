import { useTranslation } from "react-i18next";
import { useSettings } from "@/hooks/useSettings";
import type { ChineseScript, VadBackend } from "@/bindings";
import { Dropdown } from "../ui/Dropdown";
import { SettingContainer } from "../ui/SettingContainer";
import { ToggleSwitch } from "../ui/ToggleSwitch";

export function CaptureDecodeSettings() {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  return (
    <>
      <SettingContainer
        title={t("handySync.vadTitle")}
        description={t("handySync.vadDescription")}
        grouped
      >
        <Dropdown
          ariaLabel={t("handySync.vadTitle")}
          selectedValue={getSetting("vad_backend") ?? "silero"}
          disabled={isUpdating("vad_backend")}
          onSelect={(value) =>
            void updateSetting("vad_backend", value as VadBackend)
          }
          options={[
            { value: "silero", label: "Silero" },
            { value: "earshot", label: "Earshot" },
          ]}
        />
      </SettingContainer>
      <ToggleSwitch
        label={t("handySync.chunkingTitle")}
        description={t("handySync.chunkingDescription")}
        grouped
        checked={getSetting("long_form_chunking") ?? true}
        isUpdating={isUpdating("long_form_chunking")}
        onChange={(value) => void updateSetting("long_form_chunking", value)}
      />
    </>
  );
}

export function ChineseScriptSetting() {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  return (
    <SettingContainer
      title={t("handySync.chineseTitle")}
      description={t("handySync.chineseDescription")}
      grouped
    >
      <Dropdown
        ariaLabel={t("handySync.chineseTitle")}
        selectedValue={getSetting("chinese_script") ?? "as_transcribed"}
        disabled={isUpdating("chinese_script")}
        onSelect={(value) =>
          void updateSetting("chinese_script", value as ChineseScript)
        }
        options={[
          { value: "as_transcribed", label: t("handySync.asTranscribed") },
          { value: "simplified", label: t("handySync.simplified") },
          { value: "traditional", label: t("handySync.traditional") },
        ]}
      />
    </SettingContainer>
  );
}
