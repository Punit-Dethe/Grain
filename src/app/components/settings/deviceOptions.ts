import type { TFunction } from "i18next";
import type { SourceDevice } from "@/bindings";

/** Picker options for an audio device choice: "System default (name)" first
 * (value ""), then every device, then the chosen one if it is gone. Shared by
 * Recall's sources and the talk-back output. */
export function deviceOptions(
  devices: SourceDevice[],
  selected: string,
  t: TFunction,
): { value: string; label: string }[] {
  const fallback = devices.find((device) => device.is_default);
  const options = [
    {
      value: "",
      label: fallback
        ? t("ui2.meet.sources.followDefaultNamed", { name: fallback.name })
        : t("ui2.meet.sources.followDefault"),
    },
    ...devices.map((device) => ({ value: device.id, label: device.name })),
  ];
  if (selected && !devices.some((device) => device.id === selected)) {
    options.push({
      value: selected,
      label: t("ui2.meet.sources.disconnected"),
    });
  }
  return options;
}
