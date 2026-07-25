import React from "react";
import { useSettings } from "../../../hooks/useSettings";
import { Switch } from "../../ui/Switch";
import { InfoHint } from "../../ui/InfoHint";

/**
 * [GRAIN] The on/off switch for one of Grain's OWN always-present features
 * (Snippets, Context Awareness, Agent), pinned to the top of that feature's tab.
 *
 * These three used to sit in the Extensions Overview alongside installed packs,
 * which read as if you could uninstall them and made the list of things you
 * actually chose to install harder to scan. They are not installed — they ship
 * with Grain and have a tab each — so the switch belongs where the settings it
 * governs are, one line above them.
 *
 * Deliberately a bare row, not a `SettingsGroup`: it is the tab's own header,
 * not the first item in a list of settings.
 */
export const FeatureToggle: React.FC<{
  /** The core settings flag this feature gates on. */
  settingKey: "snippets_enabled" | "context_awareness_enabled" | "agent_enabled";
  title: string;
  info: string;
}> = ({ settingKey, title, info }) => {
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const enabled = (getSetting(settingKey) as boolean | undefined) ?? false;

  return (
    <div className="flex items-center justify-between gap-4 px-1">
      <div className="flex items-center gap-2 min-w-0">
        <h2 className="text-base font-semibold tracking-tight text-ink truncate">
          {title}
        </h2>
        <InfoHint text={info} position="bottom" />
      </div>
      <Switch
        checked={enabled}
        isUpdating={isUpdating(settingKey)}
        onChange={(v) => updateSetting(settingKey, v)}
        ariaLabel={`Enable ${title}`}
      />
    </div>
  );
};
