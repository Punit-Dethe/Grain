import React from "react";
import type { AgentAutocopy, AgentPanelPosition } from "@/bindings";
import { useSettings } from "../../../hooks/useSettings";
import { Dropdown } from "../../ui/Dropdown";
import { SettingContainer } from "../../ui/SettingContainer";
import { ToggleSwitch } from "../../ui/ToggleSwitch";
import { ShortcutInput } from "../ShortcutInput";

const AUTOCOPY_OPTIONS: { value: AgentAutocopy; label: string }[] = [
  { value: "off", label: "Off" },
  { value: "first", label: "First reply only" },
  { value: "all", label: "All replies" },
];

const LOOK_OPTIONS: { value: AgentPanelPosition; label: string }[] = [
  { value: "side", label: "Side card" },
  { value: "center", label: "Center panel (beta)" },
];

/** [GRAIN] Agent settings — the rows BELOW the tool's master switch (the one
 * in Agent's feature card), rendered as one ungrouped list.
 *
 * Reply appearance, copying, follow-ups and optional screen images use existing
 * settings. Automatic field/window text capture is retired. */
export const AgentSection: React.FC = () => {
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const autocopy = getSetting("agent_autocopy") ?? "first";
  const quick = getSetting("agent_quick_enabled") ?? false;
  const screenImage = getSetting("agent_screen_image") ?? false;
  const panelPosition = getSetting("agent_panel_position") ?? "side";

  return (
    <>
      {/* 1. The key that summons it. It used to live with the capture keys,
          which is where you would look for it only if you already knew Agent
          existed — it is the first thing you want after switching Agent on. */}
      <ShortcutInput
        shortcutId="summon_agent"
        grouped
        descriptionMode="tooltip"
      />

      {/* 2. Where the reply appears. "Personalize" rather than "Position": one
          of the two options is a differently shaped surface, not the same card
          moved, so this is a choice about appearance. The setting KEY stays
          `agent_panel_position` — renaming it would migrate everyone's stored
          value to say the same thing. */}
      <SettingContainer
        title="Personalize"
        description="'Side card' is the original bottom-right reply card. 'Center panel' is the sleeker center-top surface that grows with your conversation up to a maximum height, then scrolls. The center panel is still in development."
        descriptionMode="tooltip"
        grouped
      >
        <Dropdown
          options={LOOK_OPTIONS}
          selectedValue={panelPosition}
          disabled={isUpdating("agent_panel_position")}
          onSelect={(v) =>
            updateSetting("agent_panel_position", v as AgentPanelPosition)
          }
        />
      </SettingContainer>

      {/* 3. …or no reply surface at all. */}
      <ToggleSwitch
        label="Quick Agent"
        description="Skip the reply card entirely: the reply is auto-pasted straight at your cursor (replacing any still-selected text), then the pill briefly offers 'ask follow-up' in case you need to keep going. Same summon shortcut."
        descriptionMode="tooltip"
        grouped
        checked={quick}
        isUpdating={isUpdating("agent_quick_enabled")}
        onChange={(v) => updateSetting("agent_quick_enabled", v)}
      />

      {/* 4. How replies come back to you. */}
      <SettingContainer
        title="Auto-copy replies"
        description="Copy the Agent's replies to your clipboard as they arrive: only the first reply of a session, every reply (including retries and follow-ups), or never."
        descriptionMode="tooltip"
        grouped
      >
        <Dropdown
          options={AUTOCOPY_OPTIONS}
          selectedValue={autocopy}
          disabled={isUpdating("agent_autocopy")}
          onSelect={(v) => updateSetting("agent_autocopy", v as AgentAutocopy)}
        />
      </SettingContainer>

      {/* Renders its own row (name + description from the binding). While the
          Agent is open this shortcut OVERRIDES any other Grain shortcut on the
          same keys; outside the Agent it does nothing. */}
      <ShortcutInput
        shortcutId="agent_followup"
        grouped
        descriptionMode="tooltip"
      />

      <ToggleSwitch
        label="See my screen"
        description="Send a picture of the window you summoned from, so the Agent can answer about what's actually there — a chart, a diff, an error dialog, anything with no readable text. One window only, never the whole desktop or another app; the picture is taken when you press the key, kept only for that conversation, and never saved to disk. Needs an AI model that accepts images — on one that doesn't, your request still goes through as text and you still get a reply."
        descriptionMode="tooltip"
        grouped
        checked={screenImage}
        isUpdating={isUpdating("agent_screen_image")}
        onChange={(v) => updateSetting("agent_screen_image", v)}
      />
    </>
  );
};
