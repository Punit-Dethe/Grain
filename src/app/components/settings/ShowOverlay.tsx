import React from "react";
import { useTranslation } from "react-i18next";
import { Dropdown } from "../ui/Dropdown";
import { SettingContainer } from "../ui/SettingContainer";
import { useSettings } from "../../hooks/useSettings";
import type { OverlayPosition } from "@/bindings";

interface ShowOverlayProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const ShowOverlay: React.FC<ShowOverlayProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();

    const overlayOptions = [
      { value: "bottom", label: t("settings.advanced.overlay.options.bottom") },
      { value: "top", label: t("settings.advanced.overlay.options.top") },
    ];

    const selectedPosition = (getSetting("overlay_position") ||
      "bottom") as OverlayPosition;

    return (
      <SettingContainer
        title={t("settings.advanced.overlay.title")}
        description={t("settings.advanced.overlay.description")}
        descriptionMode={descriptionMode}
        grouped={grouped}
      >
        <Dropdown
          options={[
            {
              value: "none",
              label: t("settings.advanced.overlay.options.none"),
            },
            {
              value: "minimal",
              label: t("settings.advanced.overlay.options.minimal"),
            },
            {
              value: "live",
              label: t("settings.advanced.overlay.options.live"),
            },
          ]}
          selectedValue={getSetting("overlay_style") || "live"}
          onSelect={(value) =>
            updateSetting("overlay_style", value as "none" | "minimal" | "live")
          }
          disabled={isUpdating("overlay_style")}
        />
        <Dropdown
          options={overlayOptions}
          selectedValue={selectedPosition}
          onSelect={(value) =>
            updateSetting("overlay_position", value as OverlayPosition)
          }
          disabled={isUpdating("overlay_position")}
        />
      </SettingContainer>
    );
  },
);
