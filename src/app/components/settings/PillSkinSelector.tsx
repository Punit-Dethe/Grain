import React from "react";
import { useTranslation } from "react-i18next";
import { Dropdown } from "../ui/Dropdown";
import { SettingContainer } from "../ui/SettingContainer";
import { useSettings } from "../../hooks/useSettings";
import type { PillSkin } from "@/bindings";

interface PillSkinSelectorProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

/**
 * [GRAIN] Which built-in LOOK the pill wears. This is Grain's own form (the
 * pill's geometry and how it draws your voice) — distinct from a pill *theme*,
 * which is an extension's colours painted into whichever form is selected here.
 */
export const PillSkinSelector: React.FC<PillSkinSelectorProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();

    const skinOptions = [
      { value: "wave", label: t("settings.advanced.pillSkin.options.wave") },
      {
        value: "matrix",
        label: t("settings.advanced.pillSkin.options.matrix"),
      },
    ];

    const selectedSkin = (getSetting("pill_skin") || "wave") as PillSkin;

    return (
      <SettingContainer
        title={t("settings.advanced.pillSkin.title")}
        description={t("settings.advanced.pillSkin.description")}
        descriptionMode={descriptionMode}
        grouped={grouped}
      >
        <Dropdown
          options={skinOptions}
          selectedValue={selectedSkin}
          onSelect={(value) => updateSetting("pill_skin", value as PillSkin)}
          disabled={isUpdating("pill_skin")}
        />
      </SettingContainer>
    );
  },
);
