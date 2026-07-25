import React from "react";
import { useSettings } from "../../../hooks/useSettings";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { ToggleSwitch } from "../../ui/ToggleSwitch";

export type FeatureKey =
  | "snippets_enabled"
  | "context_awareness_enabled"
  | "agent_enabled"
  | "grain_space_enabled";

/**
 * [GRAIN] One of Grain's OWN always-present features (Snippets, Context
 * Awareness, Agent), rendered as a single ungrouped list of rows.
 *
 * The feature's master switch is the FIRST ROW, in the same well and the same
 * shape as everything it governs, and its settings appear below it only while
 * it is on. Previously the switch was a page header AND the section beneath it
 * repeated the feature's name as a group title, so "Context awareness" was
 * printed twice, once as a heading over one unrelated row. A feature is not a
 * category; it is a thing you turn on, and then it has settings.
 *
 * Sub-groupings are deliberately absent. Splitting six controls into "Reply
 * surface", "Replies" and "Input & context" gave three headings to name what
 * the rows already said, and made a short list look like a long one.
 */
export const FeaturePanel: React.FC<{
  settingKey: FeatureKey;
  title: string;
  info: string;
  /** The feature's settings — shown only while it is on. */
  children?: React.ReactNode;
}> = ({ settingKey, title, info, children }) => {
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const enabled = (getSetting(settingKey) as boolean | undefined) ?? false;

  return (
    <SettingsGroup>
      <ToggleSwitch
        label={title}
        description={info}
        descriptionMode="tooltip"
        grouped
        checked={enabled}
        isUpdating={isUpdating(settingKey)}
        onChange={(v) => updateSetting(settingKey, v)}
      />
      {enabled && children}
    </SettingsGroup>
  );
};

/** Read one feature's flag without mounting a panel — for the parts of a tab
 * that live OUTSIDE the well (a rich editor, an extension anchor) and still
 * have to disappear with it. */
export const useFeatureEnabled = (key: FeatureKey): boolean => {
  const { getSetting } = useSettings();
  return (getSetting(key) as boolean | undefined) ?? false;
};
